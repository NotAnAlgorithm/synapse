// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! A/B experiment telemetry read-model (Synapse, M3 workstream A).
//!
//! The test-date governor (PRD A2) and adoption (PRD E2/E3) are meant to ship
//! as real A/B experiments with explicit *kill-criteria* — e.g. "cut the
//! deadline arm if it shows higher *total* review load (including relearning)
//! OR lower practice scores" (governor.rs module docs). To decide any of that
//! we first need the load/quality metrics measured per arm. This module is that
//! foundation and nothing more: a read-only aggregation over the revlog that
//! reports, for one experiment, which arm the collection is assigned to and the
//! metrics needed to evaluate a kill-criterion.
//!
//! It deliberately does NOT flip any feature, touch the governor/adoption code,
//! or *assign* an arm. Arm assignment is a separate concern (see the open
//! questions in the task report): here we only READ the assignment from generic
//! collection config, exactly the way the governor and adoption read their own
//! `synapse:` flags.
//!
//! ## Arm assignment (read-only)
//!
//! The arm for experiment `<name>` is stored as a JSON string under the generic
//! config key `synapse:experiment:<name>` (e.g. `"deadline"` / `"control"`).
//! Missing / malformed ⇒ `""` (unassigned), the same degrade-to-default the
//! governor and adoption read-models use. Writing that key (assignment) is out
//! of scope for this step.
//!
//! ## Metrics (over the revlog of the searched cards, within `window_days`)
//!
//! `search` scopes the card set (empty = whole collection); the metrics are
//! aggregated over those cards' revlog, restricted to the last `window_days`
//! (revlog ids are unix-**millis**; `window_days == 0` means all time). All the
//! signals a kill-criterion needs are already in the revlog and need NO new
//! table — mirroring the governor's note that its instrumentation "need NO new
//! table":
//!
//! - **review_count** — genuine graded reviews
//!   (`has_rating_and_affects_scheduling()`); the total-load numerator.
//! - **relearn_count** — reviews whose `review_kind` is `Relearning`; the
//!   "relearning tax" the governor's kill-criterion calls out.
//! - **lapse_count** — `Again` (`button_chosen == 1`) on `Review`-kind entries;
//!   a graded card that dropped out of review.
//! - **pass_rate** — graded reviews with `button_chosen >= 2` over the graded
//!   total (`0.0` when there are none); the practice-quality signal.
//! - **total_seconds** — summed `taken_millis / 1000` over graded reviews; the
//!   time-cost side of "review load".

use anki_proto::stats::ExperimentMetricsResponse;

use crate::prelude::*;
use crate::revlog::RevlogEntry;
use crate::revlog::RevlogReviewKind;
use crate::search::SortMode;

/// Prefix for the generic-config key holding an experiment's arm assignment.
/// The full key is `synapse:experiment:<experiment>`; the value is a JSON
/// string naming the arm (`""` / absent ⇒ unassigned).
pub(crate) const EXPERIMENT_KEY_PREFIX: &str = "synapse:experiment:";

/// Metrics accumulated over a scoped, windowed slice of the revlog. Kept
/// separate from the proto response so the pure aggregation is unit-testable
/// without a `Collection`.
#[derive(Debug, Default, Clone, PartialEq)]
struct ExperimentMetrics {
    /// Genuine graded reviews (`has_rating_and_affects_scheduling()`).
    review_count: u32,
    /// Reviews in the relearning phase (`RevlogReviewKind::Relearning`).
    relearn_count: u32,
    /// `Again` (button 1) on `Review`-kind entries.
    lapse_count: u32,
    /// Graded reviews that passed (`button_chosen >= 2`); the pass-rate
    /// numerator.
    pass_count: u32,
    /// Sum of `taken_millis` over graded reviews (converted to seconds on
    /// read).
    taken_millis: u64,
}

impl ExperimentMetrics {
    /// Fold one revlog entry into the running totals. Only genuine graded
    /// reviews contribute; manual reschedules / cramming / rating-less entries
    /// are ignored, exactly as the adoption and performance read-models do.
    fn add(&mut self, entry: &RevlogEntry) {
        if !entry.has_rating_and_affects_scheduling() {
            return;
        }
        self.review_count += 1;
        self.taken_millis += entry.taken_millis as u64;
        if entry.button_chosen >= 2 {
            self.pass_count += 1;
        }
        if entry.review_kind == RevlogReviewKind::Relearning {
            self.relearn_count += 1;
        }
        // A lapse is an `Again` on a card that was in the review phase (it fell
        // out of review), distinct from an `Again` during (re)learning steps.
        if entry.review_kind == RevlogReviewKind::Review && entry.button_chosen == 1 {
            self.lapse_count += 1;
        }
    }

    /// Fraction of graded reviews that passed (`button_chosen >= 2`), or `0.0`
    /// when there were no graded reviews.
    fn pass_rate(&self) -> f32 {
        if self.review_count == 0 {
            0.0
        } else {
            self.pass_count as f32 / self.review_count as f32
        }
    }

    fn total_seconds(&self) -> f64 {
        self.taken_millis as f64 / 1000.0
    }
}

/// Aggregate the metrics over `revlog`, keeping only entries whose timestamp is
/// within the last `window_days`. Revlog ids are unix-**millis**; `now_millis`
/// is the current time in unix-millis. `window_days == 0` disables the window
/// (all time).
fn experiment_metrics_over(
    revlog: &[RevlogEntry],
    window_days: u32,
    now_millis: i64,
) -> ExperimentMetrics {
    // Cutoff (inclusive) in unix-millis; `None` ⇒ all time.
    let cutoff_millis: Option<i64> = if window_days == 0 {
        None
    } else {
        Some(now_millis - window_days as i64 * 86_400_000)
    };
    let mut metrics = ExperimentMetrics::default();
    for entry in revlog {
        if let Some(cutoff) = cutoff_millis {
            if entry.id.0 < cutoff {
                continue;
            }
        }
        metrics.add(entry);
    }
    metrics
}

impl Collection {
    /// The arm this collection is assigned to for `experiment`, read from the
    /// generic config key `synapse:experiment:<experiment>`. `""` when unset or
    /// malformed. Read-only; assignment is intentionally not performed here.
    pub(crate) fn experiment_arm(&self, experiment: &str) -> String {
        let key = format!("{EXPERIMENT_KEY_PREFIX}{experiment}");
        self.get_config_default::<String, _>(key.as_str())
    }

    /// A/B experiment telemetry read-model over the cards matched by `search`
    /// (empty = whole collection), restricted to the last `window_days`
    /// (`0` = all time). Reports the assigned arm plus the review-load and
    /// practice-quality metrics needed to evaluate an experiment's
    /// kill-criterion. Read-only: it changes no scheduling, writes no config,
    /// and flips no feature. See the module docs for the metric definitions.
    pub(crate) fn experiment_metrics(
        &mut self,
        experiment: &str,
        window_days: u32,
        search: &str,
    ) -> Result<ExperimentMetricsResponse> {
        let arm = self.experiment_arm(experiment);

        let guard = self.search_cards_into_table(search, SortMode::NoOrder)?;
        let revlog = guard
            .col
            .storage
            .get_revlog_entries_for_searched_cards_in_card_order()?;
        drop(guard);

        let metrics = experiment_metrics_over(&revlog, window_days, TimestampMillis::now().0);
        Ok(ExperimentMetricsResponse {
            arm,
            review_count: metrics.review_count,
            relearn_count: metrics.relearn_count,
            lapse_count: metrics.lapse_count,
            pass_rate: metrics.pass_rate(),
            total_seconds: metrics.total_seconds(),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::revlog::RevlogId;

    /// A base millis for review ids, comfortably in the past (30 days), so a
    /// tighter window in a test still includes them.
    fn base_ms() -> i64 {
        TimestampMillis::now().0 - 30 * 86_400_000
    }

    /// Build a graded revlog entry for `cid` at unix-millis `id_ms`.
    fn review(cid: CardId, id_ms: i64, button: u8, kind: RevlogReviewKind) -> RevlogEntry {
        RevlogEntry {
            id: RevlogId(id_ms),
            cid,
            button_chosen: button,
            interval: 10,
            last_interval: 10,
            ease_factor: 2500,
            taken_millis: 3000,
            review_kind: kind,
            ..Default::default()
        }
    }

    // --- pure aggregation --------------------------------------------------

    #[test]
    fn metrics_counts_passes_lapses_and_relearns() {
        let cid = CardId(1);
        let b = base_ms();
        let revlog = vec![
            review(cid, b, 3, RevlogReviewKind::Review),     // pass
            review(cid, b + 1, 1, RevlogReviewKind::Review), // lapse (Again in review)
            review(cid, b + 2, 4, RevlogReviewKind::Review), // pass (Easy)
            review(cid, b + 3, 3, RevlogReviewKind::Relearning), // relearn pass
            // A manual reschedule (button 0) must be ignored entirely.
            review(cid, b + 4, 0, RevlogReviewKind::Manual),
        ];
        // All-time window.
        let m = experiment_metrics_over(&revlog, 0, TimestampMillis::now().0);

        // 4 graded reviews (the manual entry is excluded).
        assert_eq!(m.review_count, 4);
        // Two lapse-free passes + the relearning pass = 3 passes over 4 graded.
        assert_eq!(m.pass_count, 3);
        assert!((m.pass_rate() - 0.75).abs() < 1e-6, "{}", m.pass_rate());
        // One relearning entry.
        assert_eq!(m.relearn_count, 1);
        // One Again on a Review-kind entry.
        assert_eq!(m.lapse_count, 1);
        // 4 graded reviews * 3000ms = 12s (the manual entry contributes none).
        assert!(
            (m.total_seconds() - 12.0).abs() < 1e-9,
            "{}",
            m.total_seconds()
        );
    }

    #[test]
    fn relearning_again_is_not_a_lapse() {
        // An `Again` during relearning is not a fresh lapse (the card had
        // already lapsed); only `Again` on Review-kind entries counts.
        let cid = CardId(1);
        let b = base_ms();
        let revlog = vec![review(cid, b, 1, RevlogReviewKind::Relearning)];
        let m = experiment_metrics_over(&revlog, 0, TimestampMillis::now().0);
        assert_eq!(m.review_count, 1);
        assert_eq!(m.lapse_count, 0);
        assert_eq!(m.relearn_count, 1);
        // No passes ⇒ pass_rate is the graded-fraction 0.0, not a divide-by-zero.
        assert_eq!(m.pass_rate(), 0.0);
    }

    #[test]
    fn empty_revlog_has_zero_pass_rate() {
        let m = experiment_metrics_over(&[], 0, TimestampMillis::now().0);
        assert_eq!(m.review_count, 0);
        assert_eq!(m.pass_rate(), 0.0);
        assert_eq!(m.total_seconds(), 0.0);
    }

    #[test]
    fn window_days_filters_out_old_entries() {
        let cid = CardId(1);
        let now = TimestampMillis::now().0;
        let day = 86_400_000i64;
        let revlog = vec![
            review(cid, now - 40 * day, 3, RevlogReviewKind::Review), // old
            review(cid, now - 2 * day, 3, RevlogReviewKind::Review),  // recent
            review(cid, now - day, 1, RevlogReviewKind::Review),      // recent lapse
        ];

        // A 7-day window keeps only the two recent entries.
        let windowed = experiment_metrics_over(&revlog, 7, now);
        assert_eq!(windowed.review_count, 2);
        assert_eq!(windowed.lapse_count, 1);
        assert_eq!(windowed.pass_count, 1);
        assert!((windowed.pass_rate() - 0.5).abs() < 1e-6);

        // window_days == 0 disables the filter: all three entries counted.
        let all = experiment_metrics_over(&revlog, 0, now);
        assert_eq!(all.review_count, 3);
    }

    // --- arm assignment (config read) --------------------------------------

    #[test]
    fn arm_defaults_to_empty_when_unset() {
        let col = Collection::new();
        assert_eq!(col.experiment_arm("governor"), "");
    }

    #[test]
    fn arm_reads_from_config_key() -> Result<()> {
        let mut col = Collection::new();
        let key = format!("{EXPERIMENT_KEY_PREFIX}governor");
        col.set_config(key.as_str(), &"deadline".to_string())?;
        assert_eq!(col.experiment_arm("governor"), "deadline");
        // A different experiment is still unassigned (keys are namespaced).
        assert_eq!(col.experiment_arm("adoption"), "");
        Ok(())
    }

    // --- end-to-end over a Collection --------------------------------------

    #[test]
    fn experiment_metrics_reads_revlog_and_arm() -> Result<()> {
        let mut col = Collection::new();
        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        let mut note = nt.new_note();
        col.add_note(&mut note, DeckId(1))?;
        let cid = col
            .storage
            .all_card_ids_of_note_in_template_order(note.id)?[0];

        let b = base_ms();
        // Two passes, one lapse, one relearning pass on the card.
        for entry in [
            review(cid, b, 3, RevlogReviewKind::Review),
            review(cid, b + 1, 1, RevlogReviewKind::Review),
            review(cid, b + 2, 4, RevlogReviewKind::Review),
            review(cid, b + 3, 3, RevlogReviewKind::Relearning),
        ] {
            col.storage.add_revlog_entry(&entry, false)?;
        }

        // Assign an arm for this experiment.
        let key = format!("{EXPERIMENT_KEY_PREFIX}governor");
        col.set_config(key.as_str(), &"deadline".to_string())?;

        // Whole collection, all time.
        let resp = col.experiment_metrics("governor", 0, "")?;
        assert_eq!(resp.arm, "deadline");
        assert_eq!(resp.review_count, 4);
        assert_eq!(resp.relearn_count, 1);
        assert_eq!(resp.lapse_count, 1);
        assert!((resp.pass_rate - 0.75).abs() < 1e-6, "{}", resp.pass_rate);
        assert!(
            (resp.total_seconds - 12.0).abs() < 1e-9,
            "{}",
            resp.total_seconds
        );

        // A search matching no cards yields empty metrics but still the arm.
        let none = col.experiment_metrics("governor", 0, "tag:nope::missing")?;
        assert_eq!(none.arm, "deadline");
        assert_eq!(none.review_count, 0);
        assert_eq!(none.pass_rate, 0.0);
        Ok(())
    }
}
