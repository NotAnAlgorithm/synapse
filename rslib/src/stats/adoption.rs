// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Adoption / adherence read-model (PRD E2/E3).
//!
//! The goal is to reward the behaviour that actually moves score-per-hour
//! *without corrupting the objective*: a study loop that only rewarded raw
//! activity would push a learner toward easy-rep padding (grinding cards they
//! already know) to keep a number going up. So this read-model does two things:
//!
//! 1. **Difficulty-weighted points.** Each successful retrieval is weighted by
//!    how *hard* it was — a success on a card whose retrievability had drifted
//!    low (you nearly forgot it, and got it anyway) is worth a lot; a success
//!    on a card you were almost certain to recall is worth ~0. Recovering a
//!    lapsed card earns a bonus. Easy-rep padding therefore earns almost
//!    nothing.
//!
//! 2. **Streak with freeze/forgiveness.** A streak counts consecutive study
//!    days, but a *single* missed day is bridged by a "freeze" credit rather
//!    than wiping the streak — one off day shouldn't erase weeks of momentum.
//!
//! Everything here is a read-only aggregation over the revlog plus a small
//! piece of streak state in generic collection config. It changes **no**
//! scheduling and touches **no** schema. The whole feature is gated by the
//! [`ADOPTION_ENABLED_KEY`] flag (default off): when disabled we still compute
//! and return the stats (so the caller can preview them), but callers are
//! expected to check the flag before surfacing points/streaks to the user.
//!
//! ## Reconstructing retrievability at review time
//!
//! We deliberately avoid replaying full FSRS memory-state history per card
//! (expensive, and coupled to per-deck config); the revlog already carries a
//! robust, self-contained hardness signal. When FSRS schedules a card it picks
//! the interval so that *at the due date* retrievability equals the desired
//! retention `dr` (default 0.9). So the interval the card had reached before a
//! review (`last_interval`) implies an effective stability
//! `S = last_interval / factor(dr)`, where `factor(dr) = dr^(1/-decay) - 1`
//! comes straight from the FSRS forgetting curve
//! `R(t) = (t/S * factor(0.9) + 1)^(-decay)` (see fsrs
//! `current_retrievability`).
//!
//! Substituting `S` back into the curve gives a formula that only needs the
//! elapsed time, the previous interval, the decay and the desired retention:
//!
//! ```text
//! R_at_review = (elapsed/last_interval * factor(dr) + 1)^(-decay)
//! ```
//!
//! At `elapsed == last_interval` this is exactly `dr`; reviewing *later* than
//! scheduled drives `R` below `dr` (a harder win), reviewing early drives it
//! above. This needs no memory_state and is stable across cards.

use std::collections::BTreeSet;

use anki_proto::stats::adoption_stats_response::HardWin;
use anki_proto::stats::AdoptionStatsResponse;
use fsrs::FSRS5_DEFAULT_DECAY;
use serde::Deserialize;
use serde::Serialize;

use crate::prelude::*;
use crate::revlog::RevlogEntry;
use crate::revlog::RevlogReviewKind;
use crate::scheduler::timing::SchedTimingToday;
use crate::search::SortMode;

// --- Tunable constants -----------------------------------------------------
//
// These are the knobs that shape adherence incentives. They live here (not in
// config) so the point model is auditable and reproducible; promote to config
// only if per-user tuning is ever needed.

/// Generic-config flag gating the whole feature. Default off (E2/E3 ship behind
/// a flag). Stored as a JSON bool under this key.
pub(crate) const ADOPTION_ENABLED_KEY: &str = "synapse:adoption_enabled";

/// Generic-config key for the persisted streak state ([`StreakState`] as JSON).
pub(crate) const STREAK_STATE_KEY: &str = "synapse:streak";

/// Points a "maximally hard" successful retrieval (retrievability -> 0) is
/// worth before the hardness curve. A success at the target retention earns a
/// small fraction of this; a near-certain success earns ~0.
const BASE_POINTS: f32 = 10.0;

/// Exponent on the hardness term `(1 - R)`. > 1 sharpens the curve so that only
/// genuinely difficult recalls earn meaningful points and easy reps flatten to
/// ~0 (anti-padding).
const HARDNESS_EXPONENT: f32 = 2.0;

/// Flat bonus added when a successful review *recovers a lapsed card* (a
/// relearning-phase pass). Recovering something you'd forgotten is exactly the
/// honest work we want to reward.
const LAPSE_RECOVERY_BONUS: f32 = 5.0;

/// Reviews whose reconstructed retrievability is at or above this are treated
/// as "you already knew it" and earn no points at all, regardless of the curve.
/// Keeps trivial padding from accumulating rounding-dust points.
const EASY_REP_RETRIEVABILITY: f32 = 0.98;

/// Number of isolated single missed days a streak can absorb before it breaks.
/// Each such gap consumes one freeze; two *consecutive* missed days always
/// break the streak regardless of freezes left. This is the forgiveness budget
/// that keeps one off day (or two spread-out off days) from wiping momentum.
const MAX_FREEZES: u32 = 2;

/// How many "hardest wins" to surface for recognition on the dashboard.
const HARDEST_WINS_LIMIT: usize = 5;

/// Desired retention assumed when reconstructing review-time retrievability.
/// The Synapse preset keeps FSRS at its 0.9 default (roadmap A1), so this is
/// the scheduling target the intervals were built around.
const ASSUMED_DESIRED_RETENTION: f32 = 0.9;

/// Persisted streak state (generic config `synapse:streak`). Kept tiny and
/// schema-free; recomputed from the revlog on read, then written back so the
/// freeze accounting is stable across sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) struct StreakState {
    /// Day index (collection day-elapsed) of the most recent studied day that
    /// the streak counts. 0 when no streak.
    pub last_day: u32,
    /// Current streak length in days.
    pub length: u32,
    /// Freeze credits available to bridge a future single missed day.
    pub freezes_remaining: u32,
}

/// A successful retrieval, scored for the point model. Kept internal; only the
/// notable ones are surfaced as [`HardWin`]s.
struct ScoredWin {
    card_id: CardId,
    reviewed_at: TimestampSecs,
    retrievability: f32,
    points: f32,
    lapse_recovery: bool,
}

/// FSRS forgetting-curve `factor` term for a given desired retention and decay:
/// `dr^(1/-decay) - 1`. This is the same constant the fsrs crate uses inside
/// `current_retrievability`.
fn forgetting_factor(desired_retention: f32, decay: f32) -> f32 {
    desired_retention.powf(1.0 / -decay) - 1.0
}

/// Reconstruct the retrievability a card had *at the moment of a review*, from
/// the previously-scheduled interval and the actual elapsed time. See the
/// module docs for the derivation. Returns a value in `(0, 1]`.
///
/// `last_interval_secs` is the scheduled interval before this review;
/// `elapsed_secs` is the time from the previous review to this one. When we
/// lack a usable previous interval (e.g. the first graduating review), we treat
/// the review as reviewed-on-schedule (`R == desired_retention`).
fn retrievability_at_review(
    last_interval_secs: u32,
    elapsed_secs: u32,
    desired_retention: f32,
    decay: f32,
) -> f32 {
    let factor = forgetting_factor(desired_retention, decay);
    if last_interval_secs == 0 {
        // No scheduled interval to compare against; assume on-schedule.
        return desired_retention;
    }
    let ratio = elapsed_secs as f32 / last_interval_secs as f32;
    (ratio * factor + 1.0).powf(-decay).clamp(0.0, 1.0)
}

/// Points earned by a single successful retrieval given its reconstructed
/// retrievability and whether it recovered a lapsed card. A near-certain
/// success earns ~0; a low-retrievability success earns up to [`BASE_POINTS`];
/// a lapse recovery adds [`LAPSE_RECOVERY_BONUS`].
fn points_for_success(retrievability: f32, lapse_recovery: bool) -> f32 {
    let base = if retrievability >= EASY_REP_RETRIEVABILITY {
        0.0
    } else {
        BASE_POINTS * (1.0 - retrievability).powf(HARDNESS_EXPONENT)
    };
    base + if lapse_recovery {
        LAPSE_RECOVERY_BONUS
    } else {
        0.0
    }
}

/// Map a review timestamp to a collection "study day" index, using the same
/// rollover the scheduler uses. Today is `days_elapsed`; a review in the 24h
/// window ending at `next_day_at` is today, the previous window is yesterday,
/// etc. Reviews in the future (clock skew) clamp to today.
fn day_index_of(secs: TimestampSecs, timing: &SchedTimingToday) -> u32 {
    let next_day_at = timing.next_day_at.0;
    if secs.0 >= next_day_at {
        return timing.days_elapsed;
    }
    // Whole 24h windows between this review and the end of today.
    let windows_back = (next_day_at - 1 - secs.0).max(0) / 86_400;
    timing.days_elapsed.saturating_sub(windows_back as u32)
}

/// Compute streak length and remaining freeze credits from the set of distinct
/// studied day indices, walking backward from the most recent studied day with
/// single-missed-day forgiveness.
///
/// Policy (freeze budget tunable via [`MAX_FREEZES`]):
/// - Consecutive studied days extend the streak by 1 each.
/// - A **single** isolated gap day is bridged by consuming one freeze credit;
///   the streak continues across it. Two or more *consecutive* gap days always
///   end the streak (even with freezes left) — a genuine multi-day lapse should
///   reset momentum.
/// - The whole streak can absorb at most [`MAX_FREEZES`] such bridges; once
///   they're exhausted the next gap ends the streak.
/// - The streak is live if the most recent studied day is today or yesterday
///   (yesterday keeps today "recoverable" without penalty). `freezes_remaining`
///   is the budget left after the bridges already spent.
fn compute_streak(studied_days: &BTreeSet<u32>, today: u32) -> StreakState {
    // Most recent studied day; if it's older than yesterday the streak is dead.
    let Some(&last_studied) = studied_days.iter().next_back() else {
        return StreakState::default();
    };
    if today.saturating_sub(last_studied) > 1 {
        return StreakState::default();
    }

    let mut length: u32 = 0;
    let mut freezes_spent: u32 = 0;
    // Walk day-by-day backward from the most recent studied day.
    let mut cursor = last_studied;
    loop {
        if studied_days.contains(&cursor) {
            length += 1;
            if cursor == 0 {
                break;
            }
            cursor -= 1;
            continue;
        }
        // A gap day. Bridge it with a freeze iff (a) we have budget left and
        // (b) it is an *isolated* single gap (the day before it was studied).
        let prev_studied = cursor > 0 && studied_days.contains(&(cursor - 1));
        if freezes_spent < MAX_FREEZES && prev_studied {
            freezes_spent += 1;
            // Skip the bridged gap day; resume at the studied day before it.
            cursor -= 1;
        } else {
            break;
        }
    }

    StreakState {
        last_day: last_studied,
        length,
        freezes_remaining: MAX_FREEZES.saturating_sub(freezes_spent),
    }
}

impl Collection {
    /// Whether the adoption feature is enabled (generic config, default off).
    pub(crate) fn adoption_enabled(&self) -> bool {
        self.get_config_default::<bool, _>(ADOPTION_ENABLED_KEY)
    }

    /// Adoption / adherence read-model over the cards matched by `search`
    /// (empty = whole collection). Computes difficulty-weighted points and a
    /// forgiveness-aware streak, persists the streak state, and returns the
    /// notable "hard wins" for display. See the module docs for the models.
    pub(crate) fn adoption_stats(&mut self, search: &str) -> Result<AdoptionStatsResponse> {
        let guard = self.search_cards_into_table(search, SortMode::NoOrder)?;
        let revlog = guard
            .col
            .storage
            .get_revlog_entries_for_searched_cards_in_card_order()?;
        drop(guard);

        let timing = self.timing_today()?;
        self.adoption_stats_inner(&revlog, &timing)
    }

    /// Pure computation over the given revlog + timing, split out for testing.
    fn adoption_stats_inner(
        &mut self,
        revlog: &[RevlogEntry],
        timing: &SchedTimingToday,
    ) -> Result<AdoptionStatsResponse> {
        let mut points = 0.0f32;
        let mut successful_reviews = 0u32;
        let mut lapse_recoveries = 0u32;
        let mut wins: Vec<ScoredWin> = Vec::new();
        let mut studied_days: BTreeSet<u32> = BTreeSet::new();

        // Entries arrive ordered by (cid, id), so we can read the actual elapsed
        // time between a card's consecutive reviews from the timestamp delta.
        // This is the real "how long since you last saw it" that drives
        // retrievability, rather than any single stored field.
        let mut prev_card: Option<CardId> = None;
        let mut prev_secs = TimestampSecs::zero();

        for entry in revlog {
            let reviewed_at = entry.id.as_secs();
            let elapsed_secs = if prev_card == Some(entry.cid) {
                reviewed_at.elapsed_secs_since(prev_secs).max(0) as u32
            } else {
                0
            };
            prev_card = Some(entry.cid);
            prev_secs = reviewed_at;

            // Only genuine grades count toward streak/points; skip manual
            // reschedules, cramming, and rating-less entries.
            if !entry.has_rating_and_affects_scheduling() {
                continue;
            }
            studied_days.insert(day_index_of(reviewed_at, timing));

            // A retrieval "attempt" is a Review/Relearning entry; learning-phase
            // reps (brand-new cards) aren't recall of a memory, so they don't
            // earn hardness points. Failures (button 1) earn nothing.
            let is_recall_phase = matches!(
                entry.review_kind,
                RevlogReviewKind::Review | RevlogReviewKind::Relearning
            );
            let succeeded = entry.button_chosen >= 2;
            if !is_recall_phase || !succeeded {
                continue;
            }

            let lapse_recovery = entry.review_kind == RevlogReviewKind::Relearning;
            // Prefer the true elapsed time between this card's reviews; fall back
            // to the scheduled interval (reviewed-on-schedule) when we have no
            // preceding review for this card in the window.
            let elapsed = if elapsed_secs > 0 {
                elapsed_secs
            } else {
                entry.last_interval_secs()
            };
            let r = retrievability_at_review(
                entry.last_interval_secs(),
                elapsed,
                ASSUMED_DESIRED_RETENTION,
                FSRS5_DEFAULT_DECAY,
            );
            let earned = points_for_success(r, lapse_recovery);

            points += earned;
            successful_reviews += 1;
            if lapse_recovery {
                lapse_recoveries += 1;
            }
            wins.push(ScoredWin {
                card_id: entry.cid,
                reviewed_at,
                retrievability: r,
                points: earned,
                lapse_recovery,
            });
        }

        // Streak, with freeze/forgiveness, persisted to generic config.
        let streak = compute_streak(&studied_days, timing.days_elapsed);
        self.set_config(STREAK_STATE_KEY, &streak)?;
        let studied_today = studied_days.contains(&timing.days_elapsed);
        let enabled = self.adoption_enabled();

        // Surface the most valuable wins (highest points first, then hardest).
        wins.sort_by(|a, b| {
            b.points
                .partial_cmp(&a.points)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    a.retrievability
                        .partial_cmp(&b.retrievability)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
                .then(b.reviewed_at.0.cmp(&a.reviewed_at.0))
        });
        let hardest_wins = wins
            .iter()
            .take(HARDEST_WINS_LIMIT)
            .map(|w| HardWin {
                card_id: w.card_id.0,
                reviewed_at: w.reviewed_at.0,
                retrievability: w.retrievability,
                points: w.points,
                lapse_recovery: w.lapse_recovery,
            })
            .collect();

        Ok(AdoptionStatsResponse {
            points,
            successful_reviews,
            lapse_recoveries,
            streak_days: streak.length,
            freezes_remaining: streak.freezes_remaining,
            studied_today,
            hardest_wins,
            enabled,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn timing(days_elapsed: u32, next_day_at: i64) -> SchedTimingToday {
        SchedTimingToday {
            now: TimestampSecs(next_day_at - 3600),
            days_elapsed,
            next_day_at: TimestampSecs(next_day_at),
        }
    }

    // --- point model -------------------------------------------------------

    #[test]
    fn retrievability_on_schedule_equals_desired_retention() {
        // Reviewed exactly at the scheduled interval -> R == desired retention.
        let r = retrievability_at_review(10 * 86_400, 10 * 86_400, 0.9, FSRS5_DEFAULT_DECAY);
        assert!((r - 0.9).abs() < 1e-4, "r was {r}");
    }

    #[test]
    fn retrievability_drops_when_overdue() {
        // Reviewed at 3x the scheduled interval -> notably lower retrievability.
        let on_time = retrievability_at_review(10 * 86_400, 10 * 86_400, 0.9, FSRS5_DEFAULT_DECAY);
        let overdue = retrievability_at_review(10 * 86_400, 30 * 86_400, 0.9, FSRS5_DEFAULT_DECAY);
        assert!(overdue < on_time, "overdue {overdue} !< on_time {on_time}");
        assert!(overdue < 0.9);
    }

    #[test]
    fn no_previous_interval_assumes_on_schedule() {
        let r = retrievability_at_review(0, 5 * 86_400, 0.9, FSRS5_DEFAULT_DECAY);
        assert_eq!(r, 0.9);
    }

    #[test]
    fn easy_reps_earn_almost_nothing_but_hard_wins_earn_a_lot() {
        let easy = points_for_success(0.99, false);
        let target = points_for_success(0.9, false);
        let hard = points_for_success(0.4, false);
        assert_eq!(easy, 0.0, "near-certain recall should earn 0");
        assert!(
            target < 1.0,
            "a recall at the retention target earns little"
        );
        assert!(hard > target * 5.0, "a hard win dwarfs an on-target rep");
    }

    #[test]
    fn lapse_recovery_adds_a_bonus() {
        let without = points_for_success(0.5, false);
        let with = points_for_success(0.5, true);
        assert!((with - without - LAPSE_RECOVERY_BONUS).abs() < 1e-4);
    }

    // --- day bucketing -----------------------------------------------------

    #[test]
    fn day_index_buckets_by_rollover_window() {
        // next_day_at at a round boundary; days_elapsed = 100.
        let next = 100 * 86_400;
        let t = timing(100, next);
        // just before rollover -> today
        assert_eq!(day_index_of(TimestampSecs(next - 1), &t), 100);
        // start of today's window -> today
        assert_eq!(day_index_of(TimestampSecs(next - 86_400), &t), 100);
        // one second before today's window -> yesterday
        assert_eq!(day_index_of(TimestampSecs(next - 86_400 - 1), &t), 99);
        // two windows back -> two days ago
        assert_eq!(day_index_of(TimestampSecs(next - 2 * 86_400 - 1), &t), 98);
        // future review (clock skew) clamps to today
        assert_eq!(day_index_of(TimestampSecs(next + 10), &t), 100);
    }

    // --- streak with freeze/forgiveness ------------------------------------

    #[test]
    fn contiguous_days_streak() {
        let days: BTreeSet<u32> = [10, 11, 12, 13].into_iter().collect();
        let s = compute_streak(&days, 13);
        assert_eq!(s.length, 4);
        assert_eq!(s.last_day, 13);
    }

    #[test]
    fn single_missed_day_is_forgiven_with_a_freeze() {
        // Studied 8 days, then missed day 9, studied day 10 (today). The one-day
        // gap is bridged by a freeze earned over the first 7+ days.
        let days: BTreeSet<u32> = (0..=8).chain([10]).collect();
        let s = compute_streak(&days, 10);
        // day10 + bridged gap9 + days 8..0 => 10 counted days
        assert_eq!(s.length, 10, "streak was {}", s.length);
    }

    #[test]
    fn two_consecutive_missed_days_break_streak() {
        // Studied days 0..=8, missed 9 and 10, studied 11 (today). Two-day gap
        // can't be bridged -> the streak is just today.
        let days: BTreeSet<u32> = (0..=8).chain([11]).collect();
        let s = compute_streak(&days, 11);
        assert_eq!(s.length, 1, "streak was {}", s.length);
    }

    #[test]
    fn stale_last_study_resets_streak() {
        // Most recent study was 3 days ago -> streak is dead.
        let days: BTreeSet<u32> = [5, 6, 7].into_iter().collect();
        let s = compute_streak(&days, 10);
        assert_eq!(s, StreakState::default());
    }

    #[test]
    fn isolated_gaps_are_bridged_and_consume_freezes() {
        // Two separate single-day gaps (missed 9 and missed 6), both isolated,
        // are bridged out of the freeze budget. Days studied: 5,7,8,10 today 10.
        let days: BTreeSet<u32> = [5, 7, 8, 10].into_iter().collect();
        let s = compute_streak(&days, 10);
        // 10 + bridge(9) + 8 + 7 + bridge(6) + 5 = 4 studied days counted.
        assert_eq!(s.length, 4, "streak was {}", s.length);
        // Both freezes were spent bridging the two isolated gaps.
        assert_eq!(s.freezes_remaining, 0);
    }

    #[test]
    fn freeze_budget_is_finite() {
        // Three isolated single-day gaps: 2,4,6,8,10 studied (today 10), gaps at
        // 9,7,5. Only MAX_FREEZES (2) gaps can be bridged; the third ends it.
        let days: BTreeSet<u32> = [2, 4, 6, 8, 10].into_iter().collect();
        let s = compute_streak(&days, 10);
        // 10 + bridge9 + 8 + bridge7 + 6 -> stop at gap 5 (budget exhausted).
        assert_eq!(s.length, 3, "streak was {}", s.length);
        assert_eq!(s.freezes_remaining, 0);
    }

    #[test]
    fn unspent_freezes_are_reported() {
        // A clean contiguous streak spends no freezes.
        let days: BTreeSet<u32> = [8, 9, 10].into_iter().collect();
        let s = compute_streak(&days, 10);
        assert_eq!(s.length, 3);
        assert_eq!(s.freezes_remaining, MAX_FREEZES);
    }

    #[test]
    fn yesterday_keeps_streak_live() {
        // Last studied yesterday (day 9, today 10) -> streak still counts.
        let days: BTreeSet<u32> = [7, 8, 9].into_iter().collect();
        let s = compute_streak(&days, 10);
        assert_eq!(s.length, 3);
        assert_eq!(s.last_day, 9);
    }

    // --- end-to-end over a Collection --------------------------------------

    #[test]
    fn adoption_stats_scores_and_persists_streak() -> Result<()> {
        use crate::revlog::RevlogId;

        let mut col = Collection::new();
        let nt = col.get_notetype_by_name("Basic")?.unwrap();

        // Two separate cards so their timelines don't interleave: a "hard" card
        // reviewed long after its scheduled interval, and an "easy" card
        // reviewed on schedule.
        let mut hard_note = nt.new_note();
        col.add_note(&mut hard_note, DeckId(1))?;
        let hard_cid = col
            .storage
            .all_card_ids_of_note_in_template_order(hard_note.id)?[0];
        let mut easy_note = nt.new_note();
        col.add_note(&mut easy_note, DeckId(1))?;
        let easy_cid = col
            .storage
            .all_card_ids_of_note_in_template_order(easy_note.id)?[0];

        let t = col.timing_today()?;
        // A review "today" (within today's rollover window).
        let today_secs = t.next_day_at.adding_secs(-3600);
        let day = 86_400i64;

        // HARD card: a first recall on schedule (10d after a 10d interval), then a
        // second recall that is badly overdue (~90d elapsed vs a 10d schedule) ->
        // low reconstructed retrievability -> a hard win.
        let hard_first = RevlogEntry {
            id: RevlogId(today_secs.adding_secs(-90 * day).as_millis().0),
            cid: hard_cid,
            button_chosen: 3,
            interval: 10,
            last_interval: 10,
            ease_factor: 2500,
            taken_millis: 3000,
            review_kind: RevlogReviewKind::Review,
            ..Default::default()
        };
        let hard_overdue = RevlogEntry {
            id: RevlogId(today_secs.as_millis().0),
            cid: hard_cid,
            button_chosen: 3,
            interval: 30,
            last_interval: 10,
            ease_factor: 2500,
            taken_millis: 5000,
            review_kind: RevlogReviewKind::Review,
            ..Default::default()
        };

        // EASY card: two recalls, each on schedule (10d elapsed vs 10d) -> high
        // retrievability -> ~0 points.
        let easy_first = RevlogEntry {
            id: RevlogId(today_secs.adding_secs(-10 * day).as_millis().0),
            cid: easy_cid,
            button_chosen: 3,
            interval: 10,
            last_interval: 10,
            ease_factor: 2500,
            taken_millis: 2000,
            review_kind: RevlogReviewKind::Review,
            ..Default::default()
        };
        let easy_second = RevlogEntry {
            id: RevlogId(today_secs.as_millis().0 + 1),
            cid: easy_cid,
            button_chosen: 3,
            interval: 10,
            last_interval: 10,
            ease_factor: 2500,
            taken_millis: 2000,
            review_kind: RevlogReviewKind::Review,
            ..Default::default()
        };

        // A lapse recovery (relearning pass) on the easy card, today.
        let recovery = RevlogEntry {
            id: RevlogId(today_secs.as_millis().0 + 2),
            cid: easy_cid,
            button_chosen: 3,
            interval: 1,
            last_interval: 1,
            ease_factor: 2000,
            taken_millis: 3000,
            review_kind: RevlogReviewKind::Relearning,
            ..Default::default()
        };

        // A failure earns nothing but still counts as a study day.
        let fail = RevlogEntry {
            id: RevlogId(today_secs.as_millis().0 + 3),
            cid: easy_cid,
            button_chosen: 1,
            interval: 1,
            last_interval: 10,
            ease_factor: 2000,
            taken_millis: 4000,
            review_kind: RevlogReviewKind::Review,
            ..Default::default()
        };

        // Ordered by (cid, id) as the storage layer would return them.
        let revlog = vec![
            hard_first,
            hard_overdue,
            easy_first,
            easy_second,
            recovery,
            fail,
        ];
        let resp = col.adoption_stats_inner(&revlog, &t)?;

        // Successful recall attempts: hard_first, hard_overdue, easy_first,
        // easy_second, recovery = 5. The failure is excluded.
        assert_eq!(resp.successful_reviews, 5);
        assert_eq!(resp.lapse_recoveries, 1);
        assert!(resp.points > 0.0);
        assert!(resp.studied_today);
        assert_eq!(resp.streak_days, 1);

        // hardest_wins is populated and sorted by descending points.
        assert!(!resp.hardest_wins.is_empty());
        for pair in resp.hardest_wins.windows(2) {
            assert!(pair[0].points >= pair[1].points);
        }

        // The overdue hard win on the hard card is surfaced, with a
        // reconstructed retrievability well below the on-schedule 0.9 and points
        // far above an on-schedule easy rep.
        let hard_win = resp
            .hardest_wins
            .iter()
            .find(|w| w.card_id == hard_cid.0 && !w.lapse_recovery)
            .expect("overdue hard win surfaced");
        assert!(
            hard_win.retrievability < 0.7,
            "hard win R was {}",
            hard_win.retrievability
        );
        let easy_points = points_for_success(0.9, false);
        assert!(
            hard_win.points > easy_points * 3.0,
            "hard win {} not >> easy rep {}",
            hard_win.points,
            easy_points
        );

        // The lapse recovery is also surfaced and carries the recovery bonus.
        assert!(resp.hardest_wins.iter().any(|w| w.lapse_recovery));

        // Streak state was persisted to generic config.
        let persisted: StreakState = col.get_config_default(STREAK_STATE_KEY);
        assert_eq!(persisted.length, 1);
        Ok(())
    }

    #[test]
    fn adoption_enabled_flag_defaults_off() -> Result<()> {
        let mut col = Collection::new();
        assert!(!col.adoption_enabled());
        col.set_config_json(ADOPTION_ENABLED_KEY, &true, false)?;
        assert!(col.adoption_enabled());
        Ok(())
    }

    #[test]
    fn response_reflects_enabled_flag() -> Result<()> {
        let mut col = Collection::new();
        let t = col.timing_today()?;
        // Default off: read-model still computes, but `enabled` is false.
        let resp = col.adoption_stats_inner(&[], &t)?;
        assert!(!resp.enabled);

        col.set_config_json(ADOPTION_ENABLED_KEY, &true, false)?;
        let resp = col.adoption_stats_inner(&[], &t)?;
        assert!(resp.enabled);
        Ok(())
    }
}
