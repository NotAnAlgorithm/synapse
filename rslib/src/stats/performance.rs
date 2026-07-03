// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! PROVISIONAL Performance read-model (PRD F2).
//!
//! Memory (F1, `stats/concepts.rs`) answers "can you RECALL this concept";
//! Performance (F2) answers the different, harder question "can you APPLY it to
//! a novel, exam-style item". Because recall does not equal transfer (Pan &
//! Rickard put retrieval-practice transfer at d ≈ 0.28–0.40, collapsing toward
//! zero when recall is weak), Performance is a *separate* number driven by the
//! learner's answer history on APPLICATION-form cards, not plain recall cards.
//!
//! ## Provisional, uncalibrated by design
//!
//! PRD F2's success criterion is a *calibrated* probability ("a stated 70% ≈
//! 70% observed on held-out application items"). We have no held-out AAMC data
//! to calibrate against yet, so this read-model computes a **transparent,
//! uncalibrated blend** of the three signals F2 names — application accuracy,
//! current retrievability, and prerequisite mastery — and the UI labels it
//! clearly as preliminary. Real calibration (ECE/reliability) is a later
//! service concern; this module deliberately does not pretend to it.
//!
//! ## The provisional blend (per concept)
//!
//! Grouped by the `concept::<section>::<id>` tag exactly like `concepts.rs`:
//!
//! - **application_accuracy (A)** — from the revlog of the concept's
//!   application cards, the fraction of reviews that were *not* "Again"
//!   (`button_chosen >= 2`), with a mild recency weighting so recent attempts
//!   count for more (a learner improving over time should not be dragged down
//!   forever by early misses). See [`recency_weight`].
//! - **retrievability (R)** — mean FSRS current retrievability over the
//!   concept's application cards, computed the same way as `concepts.rs`.
//! - **prereq_mastery (P)** — mean F1 Memory (0..1) of the concept's
//!   prerequisites from the `concept_edges` graph. A concept with no
//!   prerequisites (or whose prerequisites have no studyable cards) gets `P =
//!   1.0` — no penalty. Weak prerequisites cap the score.
//! - **performance** = `100 * A * (0.5 + 0.5*R) * P`.
//!
//! The `(0.5 + 0.5*R)` term means retrievability *modulates* rather than
//! dominates: even at R = 0 a concept keeps half of its accuracy-driven score
//! (you have demonstrably applied it, you just may not recall it right now),
//! while high R lifts it back to full. Multiplying by `P` lets an unmet
//! prerequisite cap the whole number, encoding F2's "leans on the prerequisite
//! graph to set priors" note.
//!
//! `sufficient_data = applied_count >= 3` mirrors the F1 abstention threshold.
//! Per the owner decision we STILL return the provisional score when it is
//! false; the flag exists so the UI can label thin data honestly (F4).

use std::collections::HashMap;
use std::collections::HashSet;

use anki_proto::stats::concept_performance_response::ConceptScore;
use anki_proto::stats::ConceptPerformanceResponse;
use fsrs::FSRS;
use fsrs::FSRS5_DEFAULT_DECAY;

use crate::prelude::*;
use crate::revlog::RevlogEntry;
use crate::scheduler::timing::SchedTimingToday;
use crate::search::SortMode;

/// Minimum number of application reviews before a concept's Performance is
/// trusted (F4 abstention). Matches the F1 threshold in `concepts.rs`.
const SUFFICIENT_DATA_THRESHOLD: u32 = 3;

/// Notetype names whose cards count as APPLICATION-form (F2 is transfer, not
/// recall). Recall/Basic cards contribute nothing to application accuracy. Kept
/// as a small static list so the definition of "application card" is auditable
/// in one place; extend as the Synapse notetype catalogue grows.
const APPLICATION_NOTETYPES: &[&str] = &[
    "MCAT Application",
    "MCAT Which-Principle",
    "MCAT Data-Snippet",
    "MCAT Explain-Why",
];

/// Recency-weight for the k-th-most-recent review of a card (k = 0 is the most
/// recent). We weight each review by `RECENCY_BASE^k`, an exponential decay so
/// recent attempts dominate without older ones vanishing entirely. Chosen so a
/// review ~5 attempts old still carries ~half the weight of the latest one
/// (`0.87^5 ≈ 0.5`): enough smoothing to be stable, enough recency to reflect a
/// learner who is improving.
const RECENCY_BASE: f32 = 0.87;

fn recency_weight(reviews_newer: usize) -> f32 {
    RECENCY_BASE.powi(reviews_newer as i32)
}

#[derive(Default)]
struct PerfAccumulator {
    /// The concept's section (2nd `::` segment of its tag).
    section: String,
    /// Recency-weighted sum of "not Again" outcomes over application reviews.
    weighted_correct: f32,
    /// Recency-weighted sum of all application reviews (the accuracy divisor).
    weighted_total: f32,
    /// Unweighted count of application reviews (drives the abstention flag).
    applied_count: u32,
    /// Sum of FSRS retrievability (0..1) over the concept's *application* cards
    /// that have a memory state.
    retrievability_sum: f32,
    /// Application cards whose memory_state contributed to
    /// `retrievability_sum`.
    scored_card_count: u32,
    /// Prerequisite concept tags of this concept (from `concept_edges`).
    prereqs: Vec<String>,
}

impl Collection {
    /// Per-concept provisional Performance scores (PRD F2), grouped by the
    /// `concept::<section>::<id>` layer, over the cards matched by `search`
    /// (empty = whole collection). Sibling read-model to
    /// [`Collection::concept_memory`]; see the module docs for the (explicitly
    /// uncalibrated) blend.
    pub(crate) fn concept_performance(
        &mut self,
        search: &str,
    ) -> Result<ConceptPerformanceResponse> {
        // Prerequisite mastery reads F1 Memory over the *whole* collection, not
        // the scoped view: a scoped Performance page should still know how well
        // the learner actually knows the (possibly out-of-scope) prerequisites.
        // Computed before scoping the card table so it isn't affected by the
        // `search` guard below.
        let prereq_memory = self.prereq_memory_map()?;

        let guard = self.search_cards_into_table(search, SortMode::NoOrder)?;
        let revlog = guard
            .col
            .storage
            .get_revlog_entries_for_searched_cards_in_card_order()?;
        guard.col.concept_performance_inner(&revlog, &prereq_memory)
    }

    /// F1 Memory (0..1) of every concept that has at least one card, keyed by
    /// concept tag. Used to score prerequisite mastery. Concepts with cards but
    /// no scored review return 0.0 (has-cards but unproven ⇒ a real penalty);
    /// concepts with *no* cards are absent from the map and treated by the
    /// caller as "not applicable" (no penalty), mirroring the mastery model in
    /// `storage/concept/mastery.rs`.
    fn prereq_memory_map(&mut self) -> Result<HashMap<String, f32>> {
        let memory = self.concept_memory("")?;
        Ok(memory
            .concepts
            .into_iter()
            .map(|c| (c.concept, c.memory / 100.0))
            .collect())
    }

    fn concept_performance_inner(
        &mut self,
        revlog: &[RevlogEntry],
        prereq_memory: &HashMap<String, f32>,
    ) -> Result<ConceptPerformanceResponse> {
        let timing = self.timing_today()?;
        let timing = SchedTimingToday {
            days_elapsed: timing.days_elapsed,
            now: TimestampSecs::now(),
            next_day_at: timing.next_day_at,
        };
        let fsrs = FSRS::new(None)?;

        // The set of card ids that belong to an application-form notetype. F2
        // only counts these; recall/Basic cards are excluded entirely.
        let application_cards = self.application_card_ids()?;

        // Per application card: current FSRS retrievability (only when it has a
        // memory state). Computed once per card, like `concepts.rs`.
        let cards = self.storage.all_searched_cards()?;
        let mut retrievability: HashMap<CardId, f32> = HashMap::new();
        for card in &cards {
            if !application_cards.contains(&card.id()) {
                continue;
            }
            if let Some(state) = card.memory_state {
                let elapsed_seconds = card.seconds_since_last_review(&timing).unwrap_or_default();
                let r = fsrs.current_retrievability_seconds(
                    state.into(),
                    elapsed_seconds,
                    card.decay.unwrap_or(FSRS5_DEFAULT_DECAY),
                );
                retrievability.insert(card.id(), r);
            }
        }

        // full concept tag -> accumulator, populated from the card->concept
        // mapping restricted to application cards; and the inverse card -> its
        // concept tags, so a card's reviews can be folded into each concept it
        // maps to. Both come from a single pass over card_concept_tags_in_search
        // (already scoped to the searched cards).
        let mut concepts: HashMap<String, PerfAccumulator> = HashMap::new();
        let mut card_tags: HashMap<CardId, Vec<String>> = HashMap::new();
        for row in self.storage.card_concept_tags_in_search()? {
            if !application_cards.contains(&row.card_id) {
                continue;
            }
            let entry = concepts.entry(row.tag.clone()).or_default();
            if entry.section.is_empty() {
                entry.section = row.section;
            }
            if let Some(r) = retrievability.get(&row.card_id) {
                entry.retrievability_sum += r;
                entry.scored_card_count += 1;
            }
            card_tags.entry(row.card_id).or_default().push(row.tag);
        }

        // application accuracy from the revlog. Entries arrive ordered by
        // (cid, id); we buffer each card's application reviews so we can weight
        // them by recency (newest first) before folding into its concepts.
        // Only genuine graded reviews count (skip manual reschedules/cramming).
        let mut card_reviews: HashMap<CardId, Vec<bool>> = HashMap::new();
        for entry in revlog {
            if !application_cards.contains(&entry.cid) {
                continue;
            }
            if !entry.has_rating_and_affects_scheduling() {
                continue;
            }
            // "Again" is button 1; anything >= 2 (Hard/Good/Easy) is a pass.
            let passed = entry.button_chosen >= 2;
            card_reviews.entry(entry.cid).or_default().push(passed);
        }

        // Fold each application card's reviews into every concept it maps to.
        for (card_id, outcomes) in &card_reviews {
            let Some(tags) = card_tags.get(card_id) else {
                // A review on an application card whose note carries no concept
                // tag: nothing to attribute it to.
                continue;
            };
            let n = outcomes.len();
            for tag in tags {
                let Some(entry) = concepts.get_mut(tag) else {
                    continue;
                };
                // outcomes are in chronological order; the last element is the
                // most recent, so weight index (n-1-i) as "reviews newer".
                for (i, &passed) in outcomes.iter().enumerate() {
                    let w = recency_weight(n - 1 - i);
                    entry.weighted_total += w;
                    if passed {
                        entry.weighted_correct += w;
                    }
                }
                entry.applied_count += n as u32;
            }
        }

        // Resolve concept ids -> tags once so prerequisite ids can be rendered
        // back to tags without re-querying per concept (mirrors graph.rs).
        let id_to_tag: HashMap<i64, String> = self
            .storage
            .all_concepts()?
            .into_iter()
            .map(|c| (c.id.0, c.tag))
            .collect();

        // Attach each concept's prerequisites so the score can be capped.
        for (tag, entry) in concepts.iter_mut() {
            entry.prereqs = self.prerequisite_tags_for_concept(tag, &id_to_tag)?;
        }

        let mut scores: Vec<ConceptScore> = concepts
            .into_iter()
            .map(|(concept, acc)| {
                let application_accuracy = if acc.weighted_total > 0.0 {
                    acc.weighted_correct / acc.weighted_total
                } else {
                    0.0
                };
                let retrievability = if acc.scored_card_count > 0 {
                    acc.retrievability_sum / acc.scored_card_count as f32
                } else {
                    0.0
                };
                let prereq_mastery = mean_prereq_mastery(&acc.prereqs, prereq_memory);
                // Provisional blend: weak prerequisites cap the score; low
                // retrievability halves (not zeroes) demonstrated accuracy.
                let performance =
                    100.0 * application_accuracy * (0.5 + 0.5 * retrievability) * prereq_mastery;
                ConceptScore {
                    concept,
                    section: acc.section,
                    performance,
                    application_accuracy,
                    applied_count: acc.applied_count,
                    retrievability,
                    prereq_mastery,
                    sufficient_data: acc.applied_count >= SUFFICIENT_DATA_THRESHOLD,
                }
            })
            .collect();
        // Deterministic ordering by concept tag.
        scores.sort_by(|a, b| a.concept.cmp(&b.concept));

        Ok(ConceptPerformanceResponse { concepts: scores })
    }

    /// The set of card ids belonging to an application-form notetype (see
    /// [`APPLICATION_NOTETYPES`]). Resolved by matching notetype names, then
    /// collecting the cards of notes with those notetypes. Empty when the
    /// collection has none of the application notetypes.
    fn application_card_ids(&mut self) -> Result<HashSet<CardId>> {
        let app_ntids: HashSet<NotetypeId> = self
            .storage
            .get_all_notetype_names()?
            .into_iter()
            .filter(|(_, name)| APPLICATION_NOTETYPES.contains(&name.as_str()))
            .map(|(id, _)| id)
            .collect();
        if app_ntids.is_empty() {
            return Ok(HashSet::new());
        }

        // note id -> is-application, then every card of an application note.
        let app_notes: HashSet<NoteId> = self
            .storage
            .all_note_ids_by_notetype()?
            .into_iter()
            .filter(|(ntid, _)| app_ntids.contains(ntid))
            .map(|(_, nid)| nid)
            .collect();

        let mut ids = HashSet::new();
        for nid in &app_notes {
            for cid in self.storage.all_card_ids_of_note_in_template_order(*nid)? {
                ids.insert(cid);
            }
        }
        Ok(ids)
    }

    /// Prerequisite concept tags of `concept` from the `concept_edges` graph
    /// (`from` is a prerequisite of `to`), resolved from ids back to tags via
    /// the caller's `id_to_tag` map. Empty when the concept is unknown or has
    /// no prerequisites.
    fn prerequisite_tags_for_concept(
        &self,
        concept: &str,
        id_to_tag: &HashMap<i64, String>,
    ) -> Result<Vec<String>> {
        let Some(id) = self.storage.get_concept_id_by_tag(concept)? else {
            return Ok(Vec::new());
        };
        Ok(self
            .storage
            .get_prerequisites(id)?
            .into_iter()
            .filter_map(|prereq| id_to_tag.get(&prereq.0).cloned())
            .collect())
    }
}

/// Mean F1 Memory (0..1) over the given prerequisite tags. Prerequisites absent
/// from `prereq_memory` (no studyable cards) are "not applicable" and skipped;
/// when *no* prerequisite is applicable — including a concept with no
/// prerequisites at all — the result is `1.0` (no penalty), per PRD F2.
fn mean_prereq_mastery(prereqs: &[String], prereq_memory: &HashMap<String, f32>) -> f32 {
    let mut sum = 0.0f32;
    let mut count = 0u32;
    for tag in prereqs {
        if let Some(&mem) = prereq_memory.get(tag) {
            sum += mem;
            count += 1;
        }
    }
    if count == 0 {
        1.0
    } else {
        sum / count as f32
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::card::FsrsMemoryState;
    use crate::revlog::RevlogId;
    use crate::revlog::RevlogReviewKind;

    /// Ensure an application-form notetype exists by cloning "Basic" under one
    /// of the [`APPLICATION_NOTETYPES`] names, and return it.
    fn application_notetype(col: &mut Collection) -> Notetype {
        let mut nt = col.basic_notetype();
        nt.id = NotetypeId(0);
        nt.name = "MCAT Application".to_string();
        col.add_notetype(&mut nt, true).unwrap();
        nt
    }

    /// Add a note of `nt` tagged with `tags`, returning its first card id.
    fn add_card(col: &mut Collection, nt: &Notetype, front: &str, tags: &[&str]) -> CardId {
        let mut note = nt.new_note();
        note.set_field(0, front).unwrap();
        note.tags = tags.iter().map(|t| (*t).to_string()).collect();
        col.add_note(&mut note, DeckId(1)).unwrap();
        col.storage
            .all_card_ids_of_note_in_template_order(note.id)
            .unwrap()[0]
    }

    /// Append a graded review to a card's revlog with the given pass/fail.
    fn add_review(col: &mut Collection, cid: CardId, id_ms: i64, passed: bool) {
        let entry = RevlogEntry {
            id: RevlogId(id_ms),
            cid,
            button_chosen: if passed { 3 } else { 1 },
            interval: 10,
            last_interval: 10,
            ease_factor: 2500,
            taken_millis: 3000,
            review_kind: RevlogReviewKind::Review,
            ..Default::default()
        };
        col.storage.add_revlog_entry(&entry, false).unwrap();
    }

    /// Give a card a fresh FSRS memory state so it has a retrievability signal.
    fn give_memory_state(col: &mut Collection, cid: CardId) {
        let mut card = col.storage.get_card(cid).unwrap().unwrap();
        card.memory_state = Some(FsrsMemoryState {
            stability: 100.0,
            difficulty: 5.0,
        });
        card.last_review_time = Some(TimestampSecs::now());
        col.storage.update_card(&card).unwrap();
    }

    /// A base millis for review ids, comfortably in the past.
    fn base_ms() -> i64 {
        TimestampSecs::now().adding_secs(-30 * 86_400).as_millis().0
    }

    #[test]
    fn accuracy_from_application_revlog_excludes_recall_cards() -> Result<()> {
        let mut col = Collection::new();
        let app = application_notetype(&mut col);
        let basic = col.basic_notetype();

        // Application card for the concept: three passes, one Again -> the
        // recency-weighted accuracy should be high but below 1.
        let app_cid = add_card(&mut col, &app, "apply", &["concept::biochem::amino"]);
        let b = base_ms();
        add_review(&mut col, app_cid, b, false); // oldest: Again
        add_review(&mut col, app_cid, b + 1000, true);
        add_review(&mut col, app_cid, b + 2000, true);
        add_review(&mut col, app_cid, b + 3000, true); // newest: pass

        // A RECALL (Basic) card for the SAME concept, all Again. It must NOT
        // drag the concept's application accuracy down (F2 excludes recall).
        let recall_cid = add_card(&mut col, &basic, "recall", &["concept::biochem::amino"]);
        add_review(&mut col, recall_cid, b + 10, false);
        add_review(&mut col, recall_cid, b + 20, false);

        let resp = col.concept_performance("")?;
        assert_eq!(resp.concepts.len(), 1, "only the application card counts");
        let amino = &resp.concepts[0];
        assert_eq!(amino.concept, "concept::biochem::amino");
        assert_eq!(amino.section, "biochem");
        // 4 application reviews counted; the 2 recall reviews excluded.
        assert_eq!(amino.applied_count, 4);
        assert!(amino.sufficient_data);
        // Recency-weighted: 3 recent passes dominate the single oldest miss, so
        // accuracy is high (> the unweighted 0.75) but not perfect.
        assert!(
            amino.application_accuracy > 0.75 && amino.application_accuracy < 1.0,
            "accuracy was {}",
            amino.application_accuracy
        );
        Ok(())
    }

    #[test]
    fn respects_search_scope() -> Result<()> {
        let mut col = Collection::new();
        let app = application_notetype(&mut col);

        let amino = add_card(&mut col, &app, "a", &["concept::biochem::amino"]);
        let kin = add_card(&mut col, &app, "k", &["concept::physics::kinematics"]);
        let b = base_ms();
        for i in 0..3 {
            add_review(&mut col, amino, b + i, true);
            add_review(&mut col, kin, b + 100 + i, true);
        }

        // Scoping to one concept's tag restricts the read-model to its cards.
        let resp = col.concept_performance("tag:concept::biochem::amino")?;
        assert_eq!(resp.concepts.len(), 1);
        assert_eq!(resp.concepts[0].concept, "concept::biochem::amino");

        // A search matching nothing yields no concepts.
        let resp = col.concept_performance("tag:concept::nope::missing")?;
        assert!(resp.concepts.is_empty());
        Ok(())
    }

    #[test]
    fn weak_prerequisite_lowers_the_score() -> Result<()> {
        let mut col = Collection::new();
        let app = application_notetype(&mut col);

        // enzyme_kinetics depends (via SEED_EDGES) on amino_acid_charge and
        // protein_structure. Give the dependent a perfect application record so
        // its score is driven purely by prerequisite mastery.
        let kinetics = add_card(
            &mut col,
            &app,
            "kinetics",
            &["concept::biochem::enzyme_kinetics"],
        );
        give_memory_state(&mut col, kinetics);
        let b = base_ms();
        for i in 0..4 {
            add_review(&mut col, kinetics, b + i, true);
        }

        // Baseline: prerequisites have NO studyable cards -> not applicable ->
        // P defaults to 1.0, so the score isn't penalised.
        let baseline = col.concept_performance("tag:concept::biochem::enzyme_kinetics")?;
        let baseline_score = baseline
            .concepts
            .iter()
            .find(|c| c.concept == "concept::biochem::enzyme_kinetics")
            .expect("dependent present")
            .performance;
        assert!(baseline_score > 0.0);

        // Now give a prerequisite (amino_acid_charge) a studyable card with a
        // WEAK memory (unscored -> F1 Memory 0.0). This drags prereq_mastery
        // below 1.0 and must lower the dependent's Performance, even though its
        // own application accuracy is unchanged. The prereq card is a recall
        // (Basic) card and out of the dependent's search scope, proving the
        // penalty comes through the prerequisite graph, not the card table.
        let basic = col.basic_notetype();
        add_card(
            &mut col,
            &basic,
            "amino",
            &["concept::biochem::amino_acid_charge"],
        );

        let penalised = col.concept_performance("tag:concept::biochem::enzyme_kinetics")?;
        let entry = penalised
            .concepts
            .iter()
            .find(|c| c.concept == "concept::biochem::enzyme_kinetics")
            .expect("dependent present");
        assert!(
            entry.prereq_mastery < 1.0,
            "prereq_mastery was {}",
            entry.prereq_mastery
        );
        assert!(
            entry.performance < baseline_score,
            "penalised {} !< baseline {}",
            entry.performance,
            baseline_score
        );
        Ok(())
    }
}
