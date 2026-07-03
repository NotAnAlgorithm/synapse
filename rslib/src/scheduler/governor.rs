// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Synapse (M3, workstream A): the test-date governor (PRD A2 / A4).
//!
//! FSRS optimises for *indefinite* retention; it has no notion of a deadline.
//! For a fixed-date high-stakes exam that is a real gap: a flat retention
//! target means a student's recall keeps decaying to the same level forever,
//! rather than *peaking on test day*. The governor closes that gap. As the exam
//! date approaches, it raises the *effective* desired retention inside a final
//! window, which shortens FSRS intervals so cards are reviewed more often and
//! recall is highest when it matters. Motivated by Cepeda et al. (2008): the
//! optimal study gap shrinks as the horizon to the test shrinks.
//!
//! ## Direction discipline (PRD A1/A2 — do not get this wrong)
//!
//! The governor only ever RAISES retention, and only LATE. It never lowers the
//! retention target early. Lowering retention early to "manufacture desirable
//! difficulty" is a category error (PRD A1): it just buys lapses and pays the
//! FSRS relearning tax. So the ramp is one-directional (up), bounded, and
//! outside the final window the returned value is *byte-for-byte* the caller's
//! base retention.
//!
//! ## Design
//!
//! Everything here is pure + standalone. The curve
//! [`governor_adjusted_retention`] is a free function with no I/O so it can be
//! unit-tested exhaustively. [`Collection::synapse_governor_config`] reads the
//! two generic collection-config keys (no proto, no deck-config, no migration),
//! and [`Collection::governor_effective_retention`] is the single entry point
//! the answer path calls. When the flag is off, or the test date is unset /
//! already past (with a small margin), the base value is returned unchanged so
//! behaviour is identical to stock FSRS.
//!
//! ## Config keys (generic collection config; `synapse:` namespaced)
//!
//! - `"synapse:governor_enabled"` — bool. Master switch, DEFAULT OFF. When
//!   absent or false, the governor is a no-op.
//! - `"synapse:test_date"` — the exam date, accepted as EITHER a JSON integer
//!   (a **unix day**: days since the Unix epoch, i.e. `unix_seconds / 86_400`)
//!   OR a JSON string `"YYYY-MM-DD"` (an ISO calendar date, interpreted in the
//!   collection's local timezone). When absent or unparseable, the governor is
//!   a no-op.
//!
//! ## Instrumentation / kill-criterion (PRD A2)
//!
//! The A2 kill-criterion is "cut it if the deadline arm shows higher *total*
//! review load (including relearning) OR lower practice scores". Both inputs
//! are already recorded and need NO new table:
//!
//! - **Total review load** (incl. relearning) is the count of `revlog` rows in
//!   the window — every answer, including relearning steps, writes one. Group
//!   by `revlog.type` to separate the relearning tax. See `stats::graphs` for
//!   the existing revlog read-model to aggregate over.
//! - **Practice scores** come from the same revlog (`button_chosen` /
//!   pass-rate) and from the app-layer practice results.
//!
//! For lightweight local A/B telemetry (single-user now; cross-user later),
//! record the enabled/disabled arm and the window parameters against the
//! `"synapse:governor_*"` config namespace — no schema change needed. This
//! module deliberately does not write telemetry itself (the answer path is hot
//! and config writes are transactional); the integrator logs the arm once at
//! provisioning time.

use serde_json::Value;

use crate::prelude::*;
use crate::scheduler::timing::SchedTimingToday;

/// Generic-config key: master on/off switch for the governor. Default off.
pub const GOVERNOR_ENABLED_KEY: &str = "synapse:governor_enabled";
/// Generic-config key: the exam date (unix-day integer or `"YYYY-MM-DD"`).
pub const TEST_DATE_KEY: &str = "synapse:test_date";

// --- Curve parameters (conservative, bounded; tune against A2 outcome data) --
//
// The window is the last `RAMP_WINDOW_DAYS` days before the exam. Outside it
// the base retention is returned untouched. Inside it, effective retention
// ramps smoothly from the base up toward `MAX_RETENTION` as the date nears,
// reaching the ceiling on exam day. The ramp only ever pulls UP: if the
// caller's base is already at/above the value the curve would produce, the base
// wins (we never lower retention — PRD A1/A2).

/// Length of the pre-exam ramp window, in days. ~3 weeks (PRD A2: "final ~2-3
/// weeks"). Outside this window the governor is a no-op.
pub const RAMP_WINDOW_DAYS: u32 = 21;

/// Retention ceiling reached on exam day. Bounded well below 1.0 so intervals
/// stay finite and the relearning tax stays sane; ~0.97 per the task brief.
pub const MAX_RETENTION: f32 = 0.97;

/// Grace margin (days) for a test date in the *recent* past. If the exam is up
/// to this many days past, we still peg to the ceiling (the exam window may
/// straddle a couple of days / timezones); beyond it the governor switches off
/// so an old, forgotten test date can't pin retention high forever.
pub const PAST_GRACE_DAYS: i64 = 2;

/// The exponent shaping the ramp. `1.0` is linear; `>1.0` is convex (most of
/// the compression lands in the final days, which matches the intent of peaking
/// *on* the date rather than spreading the cost evenly). Kept mild.
const RAMP_EXPONENT: f32 = 2.0;

/// Resolved governor settings for the collection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GovernorConfig {
    pub enabled: bool,
    /// Exam date as a unix day (days since the Unix epoch). `None` when unset
    /// or unparseable.
    pub test_unix_day: Option<i64>,
}

impl GovernorConfig {
    /// Whether the governor can do anything at all (flag on and a date set).
    pub fn active(&self) -> bool {
        self.enabled && self.test_unix_day.is_some()
    }
}

/// The governor curve as a pure function, so it is fully unit-testable.
///
/// Given the caller's `base_retention` and the whole number of scheduler-days
/// until the exam (`days_to_test`, where 0 == exam day, negative == past),
/// return the effective desired retention to feed FSRS.
///
/// Guarantees (all covered by tests):
/// - Outside the window (`days_to_test > RAMP_WINDOW_DAYS`) the base is
///   returned unchanged — *byte-for-byte*, no float drift.
/// - The result never drops below `base_retention` (governor only pulls up).
/// - The result never exceeds `MAX_RETENTION`.
/// - On/after the exam day (within grace) the ceiling is applied; well past it,
///   the base is returned (handled by the caller via [`days_to_test`] sign, but
///   also guarded here).
/// - Monotonic: as `days_to_test` shrinks inside the window, the result is
///   non-decreasing.
pub fn governor_adjusted_retention(base_retention: f32, days_to_test: i64) -> f32 {
    // Past the exam beyond the grace margin: no-op (the caller normally gates
    // this out first, but keep the pure fn self-consistent).
    if days_to_test < -PAST_GRACE_DAYS {
        return base_retention;
    }
    // Outside the ramp window (too far out): byte-for-byte no-op.
    if days_to_test > RAMP_WINDOW_DAYS as i64 {
        return base_retention;
    }

    // Inside the window (including the exam day and the small grace tail).
    // progress in [0, 1]: 0 at the window's far edge, 1 on/after exam day.
    let remaining = days_to_test.max(0) as f32;
    let progress = 1.0 - (remaining / RAMP_WINDOW_DAYS as f32);
    let progress = progress.clamp(0.0, 1.0);
    let shaped = progress.powf(RAMP_EXPONENT);

    // Ramp from the base up toward the ceiling. If the base already exceeds the
    // ceiling (e.g. a user set 0.98), never pull it DOWN — clamp the target up
    // to the base so the max() below is a true no-op in that case.
    let ceiling = MAX_RETENTION.max(base_retention);
    let adjusted = base_retention + shaped * (ceiling - base_retention);

    // Belt and braces: the governor only ever raises, and stays bounded.
    adjusted.clamp(base_retention, ceiling)
}

/// Parse a `synapse:test_date` config value into a unix day.
///
/// Accepts a JSON integer (already a unix day) or a JSON string `"YYYY-MM-DD"`
/// (interpreted at the given local UTC offset, then floored to a unix day).
/// Returns `None` for any other shape or an unparseable string.
fn test_date_value_to_unix_day(value: &Value, utc_offset: chrono::FixedOffset) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => {
            let date = chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()?;
            // Midnight of that calendar day at the collection's offset, floored
            // to a whole unix day. Using the offset keeps "which day" stable
            // regardless of the wall-clock hour.
            let dt = date.and_hms_opt(0, 0, 0)?;
            let secs = dt.and_local_timezone(utc_offset).single()?.timestamp();
            Some(secs.div_euclid(86_400))
        }
        _ => None,
    }
}

impl Collection {
    /// Read the two generic-config governor keys. Never errors: a missing or
    /// malformed value degrades to "off" / "no date" so the answer path stays
    /// infallible and defaults to stock FSRS.
    pub fn synapse_governor_config(&self) -> GovernorConfig {
        let enabled = self
            .get_config_optional::<bool, _>(GOVERNOR_ENABLED_KEY)
            .unwrap_or(false);
        let utc_offset = TimestampSecs::now()
            .local_utc_offset()
            .unwrap_or_else(|_| chrono::FixedOffset::east_opt(0).unwrap());
        let test_unix_day = self
            .get_config_optional::<Value, _>(TEST_DATE_KEY)
            .and_then(|v| test_date_value_to_unix_day(&v, utc_offset));
        GovernorConfig {
            enabled,
            test_unix_day,
        }
    }

    /// The single entry point for the answer path: given the base desired
    /// retention and the current scheduler timing, return the governor-adjusted
    /// retention. Returns `base` unchanged whenever the governor is off / has
    /// no (future, in-grace) date, so `off == stock` is guaranteed here.
    pub(crate) fn governor_effective_retention(
        &self,
        base_retention: f32,
        timing: &SchedTimingToday,
    ) -> f32 {
        // Cheap early-out on the hot answer path: a single bool config read when
        // the governor is off (the common case), skipping the date parse + tz
        // lookup entirely.
        if !self
            .get_config_optional::<bool, _>(GOVERNOR_ENABLED_KEY)
            .unwrap_or(false)
        {
            return base_retention;
        }
        let Some(test_unix_day) = self.synapse_governor_config().test_unix_day else {
            return base_retention;
        };

        // "Today" as a unix day, anchored to the scheduler's rollover so it
        // agrees with how FSRS counts elapsed days. `next_day_at` is the *end*
        // of today, so today's unix day is (next_day_at - 1s) / 86_400.
        let today_unix_day = timing.next_day_at.adding_secs(-1).0.div_euclid(86_400);
        let days_to_test = test_unix_day - today_unix_day;

        governor_adjusted_retention(base_retention, days_to_test)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn off_is_byte_for_byte_stock_outside_window() {
        // Far from the date: identical float, not merely approximately equal.
        for base in [0.7_f32, 0.8, 0.85, 0.9, 0.95] {
            let far = governor_adjusted_retention(base, RAMP_WINDOW_DAYS as i64 + 1);
            assert_eq!(far.to_bits(), base.to_bits(), "base={base}");
            let very_far = governor_adjusted_retention(base, 365);
            assert_eq!(very_far.to_bits(), base.to_bits(), "base={base}");
        }
    }

    #[test]
    fn edge_of_window_is_still_stock() {
        // Exactly at the window edge, progress == 0, so still the base value.
        let base = 0.9;
        let at_edge = governor_adjusted_retention(base, RAMP_WINDOW_DAYS as i64);
        assert_eq!(at_edge.to_bits(), base.to_bits());
    }

    #[test]
    fn near_date_raises_retention() {
        let base = 0.9;
        let inside = governor_adjusted_retention(base, 7);
        assert!(inside > base, "expected raise inside window, got {inside}");
        assert!(inside <= MAX_RETENTION + f32::EPSILON);
    }

    #[test]
    fn peaks_at_ceiling_on_exam_day() {
        let base = 0.9;
        let on_day = governor_adjusted_retention(base, 0);
        assert!((on_day - MAX_RETENTION).abs() < 1e-6, "got {on_day}");
    }

    #[test]
    fn monotonic_non_decreasing_as_date_nears() {
        let base = 0.85;
        let mut prev = base;
        for d in (0..=RAMP_WINDOW_DAYS as i64).rev() {
            let r = governor_adjusted_retention(base, d);
            assert!(
                r >= prev - 1e-7,
                "not monotonic at d={d}: {r} < prev {prev}"
            );
            prev = r;
        }
    }

    #[test]
    fn never_lowers_when_base_above_ceiling() {
        // A user who set an unusually high base must never be pulled DOWN.
        let base = 0.98_f32;
        for d in [-1_i64, 0, 1, 5, 10, 21, 30] {
            let r = governor_adjusted_retention(base, d);
            assert!(r >= base - 1e-7, "lowered base at d={d}: {r} < {base}");
        }
    }

    #[test]
    fn bounded_at_ceiling() {
        let base = 0.7;
        for d in -PAST_GRACE_DAYS..=RAMP_WINDOW_DAYS as i64 {
            let r = governor_adjusted_retention(base, d);
            assert!(r <= MAX_RETENTION + 1e-6, "exceeded ceiling at d={d}: {r}");
            assert!(r >= base - 1e-7, "dropped below base at d={d}: {r}");
        }
    }

    #[test]
    fn recent_past_within_grace_pegs_to_ceiling() {
        let base = 0.9;
        let just_past = governor_adjusted_retention(base, -PAST_GRACE_DAYS);
        assert!((just_past - MAX_RETENTION).abs() < 1e-6, "got {just_past}");
    }

    #[test]
    fn well_past_is_stock() {
        let base = 0.9;
        let long_past = governor_adjusted_retention(base, -PAST_GRACE_DAYS - 1);
        assert_eq!(long_past.to_bits(), base.to_bits());
    }

    #[test]
    fn parses_unix_day_integer() {
        let off = chrono::FixedOffset::east_opt(0).unwrap();
        let v = Value::Number(20000.into());
        assert_eq!(test_date_value_to_unix_day(&v, off), Some(20000));
    }

    #[test]
    fn parses_iso_string() {
        let off = chrono::FixedOffset::east_opt(0).unwrap();
        // 1970-01-11 is 10 days after the epoch at UTC.
        let v = Value::String("1970-01-11".into());
        assert_eq!(test_date_value_to_unix_day(&v, off), Some(10));
    }

    #[test]
    fn rejects_garbage_test_date() {
        let off = chrono::FixedOffset::east_opt(0).unwrap();
        assert_eq!(
            test_date_value_to_unix_day(&Value::String("not-a-date".into()), off),
            None
        );
        assert_eq!(test_date_value_to_unix_day(&Value::Bool(true), off), None);
    }

    // --- Collection-level integration (config reading + entry point) --------

    use crate::collection::Collection;

    /// Build a timing whose "today" unix day is `today_unix_day`. `next_day_at`
    /// is the *end* of today, so we set it to the start of the following day.
    fn timing_with_today(today_unix_day: i64) -> SchedTimingToday {
        let next_day_at = TimestampSecs((today_unix_day + 1) * 86_400);
        SchedTimingToday {
            now: next_day_at.adding_secs(-3600),
            days_elapsed: 0,
            next_day_at,
        }
    }

    #[test]
    fn entry_point_off_is_stock() {
        let col = Collection::new();
        // No config set at all → governor disabled → base returned unchanged.
        let timing = timing_with_today(20_000);
        let base = 0.9_f32;
        let out = col.governor_effective_retention(base, &timing);
        assert_eq!(out.to_bits(), base.to_bits());
    }

    #[test]
    fn entry_point_enabled_but_no_date_is_stock() {
        let mut col = Collection::new();
        col.set_config(GOVERNOR_ENABLED_KEY, &true).unwrap();
        let timing = timing_with_today(20_000);
        let base = 0.9_f32;
        assert_eq!(
            col.governor_effective_retention(base, &timing).to_bits(),
            base.to_bits()
        );
    }

    #[test]
    fn entry_point_enabled_far_date_is_stock() {
        let mut col = Collection::new();
        col.set_config(GOVERNOR_ENABLED_KEY, &true).unwrap();
        // Exam 100 days out, well outside the ramp window.
        col.set_config(TEST_DATE_KEY, &(20_000_i64 + 100)).unwrap();
        let timing = timing_with_today(20_000);
        let base = 0.9_f32;
        assert_eq!(
            col.governor_effective_retention(base, &timing).to_bits(),
            base.to_bits()
        );
    }

    #[test]
    fn entry_point_near_date_raises() {
        let mut col = Collection::new();
        col.set_config(GOVERNOR_ENABLED_KEY, &true).unwrap();
        // Exam 5 days out (unix-day integer form).
        col.set_config(TEST_DATE_KEY, &(20_000_i64 + 5)).unwrap();
        let timing = timing_with_today(20_000);
        let base = 0.9_f32;
        let out = col.governor_effective_retention(base, &timing);
        assert!(out > base, "expected raise, got {out}");
        assert!(out <= MAX_RETENTION + 1e-6);
        // Must match the pure curve for days_to_test == 5.
        assert!((out - governor_adjusted_retention(base, 5)).abs() < 1e-6);
    }

    #[test]
    fn entry_point_disabled_flag_ignores_near_date() {
        let mut col = Collection::new();
        // Flag explicitly off, but a near date is set: still stock.
        col.set_config(GOVERNOR_ENABLED_KEY, &false).unwrap();
        col.set_config(TEST_DATE_KEY, &(20_000_i64 + 3)).unwrap();
        let timing = timing_with_today(20_000);
        let base = 0.9_f32;
        assert_eq!(
            col.governor_effective_retention(base, &timing).to_bits(),
            base.to_bits()
        );
    }

    #[test]
    fn entry_point_accepts_iso_date_string() {
        let mut col = Collection::new();
        col.set_config(GOVERNOR_ENABLED_KEY, &true).unwrap();
        // Today = unix day 10 (1970-01-11); exam on 1970-01-13 == 2 days out.
        col.set_config(TEST_DATE_KEY, &"1970-01-13").unwrap();
        let timing = timing_with_today(10);
        let base = 0.9_f32;
        let out = col.governor_effective_retention(base, &timing);
        // The tz used is the machine's local offset; allow a 1-day slop from the
        // offset by asserting only that it is raised and bounded.
        assert!(out > base, "expected raise for near ISO date, got {out}");
        assert!(out <= MAX_RETENTION + 1e-6);
    }
}
