// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Per-concept mastery signal for the Synapse knowledge graph.
//!
//! This mirrors the FSRS-retrievability aggregation used by the concept-memory
//! read model (`stats/concepts.rs`) — mean current retrievability over a
//! concept's cards — but is kept independent of that module so scheduling can
//! consume it without depending on the stats/proto layer. A concept counts as
//! *mastered* once it has enough scored cards and its mean retrievability
//! clears a conservative threshold.
//!
//! The signal is table-driven: a concept's cards are found via the
//! `card_concepts` projection (schema 19), so it stays consistent with the
//! `concept::` tag source of truth.

use std::collections::HashMap;
use std::collections::HashSet;

use fsrs::FSRS;
use fsrs::FSRS5_DEFAULT_DECAY;

use super::ConceptId;
use super::SqliteStorage;
use crate::card::Card;
use crate::prelude::*;
use crate::scheduler::timing::SchedTimingToday;

/// Minimum mean retrievability (0..1) for a concept to count as mastered.
/// Conservative on purpose: gating/credit only kick in once a prerequisite is
/// quite firmly known. Tunable.
pub(crate) const MASTERY_RETRIEVABILITY_THRESHOLD: f32 = 0.9;
/// Minimum number of cards with an FSRS memory state before a concept's mastery
/// is trusted. Below this we treat the concept as still being learned. Tunable.
pub(crate) const MASTERY_MIN_SCORED_CARDS: u32 = 2;

/// Mastery summary for a single concept.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ConceptMastery {
    pub concept: ConceptId,
    /// Total cards mapped to the concept (from `card_concepts`).
    pub total_card_count: u32,
    /// Of those, how many have an FSRS memory state (i.e. have been studied).
    pub scored_card_count: u32,
    /// Mean current retrievability over the scored cards (0..1); 0 when none.
    pub mean_retrievability: f32,
    /// True once the concept clears the mastery threshold with enough data.
    pub mastered: bool,
}

impl ConceptMastery {
    /// A prerequisite with no mapped cards teaches nothing the learner can
    /// study, so it must not permanently gate downstream material. Callers
    /// treat such concepts as "not applicable" rather than unmastered.
    pub(crate) fn has_cards(&self) -> bool {
        self.total_card_count > 0
    }
}

impl SqliteStorage {
    /// Card ids mapped to `concept` via the `card_concepts` projection.
    pub(crate) fn card_ids_for_concept(&self, concept: ConceptId) -> Result<Vec<CardId>> {
        self.db
            .prepare_cached("SELECT card_id FROM card_concepts WHERE concept_id = ?")?
            .query_and_then([concept], |r| Ok(CardId(r.get(0)?)))?
            .collect()
    }
}

impl Collection {
    /// Compute the [`ConceptMastery`] for each requested concept. Concepts with
    /// no mapped cards are still returned (with `total_card_count == 0`) so
    /// callers can distinguish "nothing to learn" from "not yet mastered".
    ///
    /// Retrievability is computed the same way as the concept-memory read
    /// model: FSRS `current_retrievability_seconds` from each card's memory
    /// state and time since last review.
    pub(crate) fn concept_mastery(
        &mut self,
        concepts: &[ConceptId],
    ) -> Result<HashMap<ConceptId, ConceptMastery>> {
        let timing = self.timing_today()?;
        let timing = SchedTimingToday {
            days_elapsed: timing.days_elapsed,
            now: TimestampSecs::now(),
            next_day_at: timing.next_day_at,
        };
        let fsrs = FSRS::new(None)?;

        let mut out = HashMap::with_capacity(concepts.len());
        for &concept in concepts {
            if out.contains_key(&concept) {
                continue;
            }
            let card_ids = self.storage.card_ids_for_concept(concept)?;
            let mut total = 0u32;
            let mut scored = 0u32;
            let mut retr_sum = 0.0f32;
            for cid in card_ids {
                let Some(card) = self.storage.get_card(cid)? else {
                    continue;
                };
                total += 1;
                if let Some(r) = card_retrievability(&fsrs, &card, &timing) {
                    retr_sum += r;
                    scored += 1;
                }
            }
            let mean = if scored > 0 {
                retr_sum / scored as f32
            } else {
                0.0
            };
            let mastered =
                scored >= MASTERY_MIN_SCORED_CARDS && mean >= MASTERY_RETRIEVABILITY_THRESHOLD;
            out.insert(
                concept,
                ConceptMastery {
                    concept,
                    total_card_count: total,
                    scored_card_count: scored,
                    mean_retrievability: mean,
                    mastered,
                },
            );
        }
        Ok(out)
    }

    /// Given candidate card ids (the caller has already restricted these to NEW
    /// application-type cards), return the subset that should be *withheld* by
    /// mastery gating: those mapped to a concept with at least one *unmet*
    /// prerequisite.
    ///
    /// A prerequisite is unmet when it has cards to study (so it is genuinely
    /// part of the deck) but is not yet mastered. Prerequisites with no mapped
    /// cards are ignored so they can never permanently block downstream
    /// material. Cards with no concept, or whose concepts have no
    /// prerequisites, are never withheld.
    pub(crate) fn concept_gated_card_ids(
        &mut self,
        candidate_card_ids: &[CardId],
    ) -> Result<HashSet<CardId>> {
        // Map each candidate to its concepts, and collect the union of all
        // prerequisite concepts so mastery is computed once per prerequisite.
        let mut card_concepts: HashMap<CardId, Vec<ConceptId>> =
            HashMap::with_capacity(candidate_card_ids.len());
        let mut prereqs_of: HashMap<ConceptId, Vec<ConceptId>> = HashMap::new();
        let mut all_prereqs: Vec<ConceptId> = Vec::new();

        for &cid in candidate_card_ids {
            let concepts = self.storage.concept_ids_for_card(cid)?;
            for &concept in &concepts {
                if let std::collections::hash_map::Entry::Vacant(e) = prereqs_of.entry(concept) {
                    let prereqs = self.storage.get_prerequisites(concept)?;
                    all_prereqs.extend(prereqs.iter().copied());
                    e.insert(prereqs);
                }
            }
            card_concepts.insert(cid, concepts);
        }

        all_prereqs.sort_unstable();
        all_prereqs.dedup();
        let mastery = self.concept_mastery(&all_prereqs)?;

        // A prerequisite blocks iff it has cards to study yet isn't mastered.
        let is_unmet = |prereq: &ConceptId| {
            mastery
                .get(prereq)
                .map(|m| m.has_cards() && !m.mastered)
                .unwrap_or(false)
        };

        let mut withheld = HashSet::new();
        for (&cid, concepts) in &card_concepts {
            let gated = concepts.iter().any(|concept| {
                prereqs_of
                    .get(concept)
                    .map(|prereqs| prereqs.iter().any(is_unmet))
                    .unwrap_or(false)
            });
            if gated {
                withheld.insert(cid);
            }
        }
        Ok(withheld)
    }

    /// Convenience: is a single concept mastered? A concept with no mapped
    /// cards is *not* considered mastered here (it simply has no signal);
    /// callers that need the "nothing to learn" distinction should use
    /// [`concept_mastery`] and inspect [`ConceptMastery::has_cards`].
    #[cfg(test)]
    pub(crate) fn concept_is_mastered(&mut self, concept: ConceptId) -> Result<bool> {
        Ok(self
            .concept_mastery(&[concept])?
            .get(&concept)
            .map(|m| m.mastered)
            .unwrap_or(false))
    }
}

/// Current FSRS retrievability (0..1) of a card, or `None` if it has no memory
/// state (e.g. a brand-new card). Mirrors `stats/concepts.rs`.
fn card_retrievability(fsrs: &FSRS, card: &Card, timing: &SchedTimingToday) -> Option<f32> {
    card.memory_state.map(|state| {
        let elapsed_seconds = card.seconds_since_last_review(timing).unwrap_or_default();
        fsrs.current_retrievability_seconds(
            state.into(),
            elapsed_seconds,
            card.decay.unwrap_or(FSRS5_DEFAULT_DECAY),
        )
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::card::CardType;
    use crate::config::BoolKey;

    /// Add a note tagged with `tag` under the "Basic" notetype and return its
    /// single card id.
    fn add_concept_card(col: &mut Collection, tag: &str) -> CardId {
        let nt = col.get_notetype_by_name("Basic").unwrap().unwrap();
        let mut note = nt.new_note();
        note.set_field(0, "q").unwrap();
        note.tags = vec![tag.to_string()];
        col.add_note(&mut note, DeckId(1)).unwrap();
        col.storage
            .all_card_ids_of_note_in_template_order(note.id)
            .unwrap()[0]
    }

    /// Force a card into a strongly-remembered review state so its
    /// retrievability is ~1.0.
    fn make_well_remembered(col: &mut Collection, cid: CardId) {
        let mut card = col.storage.get_card(cid).unwrap().unwrap();
        card.ctype = CardType::Review;
        card.queue = crate::card::CardQueue::Review;
        card.interval = 30;
        card.due = 0;
        card.memory_state = Some(crate::card::FsrsMemoryState {
            stability: 100.0,
            difficulty: 5.0,
        });
        card.last_review_time = Some(TimestampSecs::now());
        let orig = col.storage.get_card(cid).unwrap().unwrap();
        col.update_card_inner(&mut card, orig, col.usn().unwrap())
            .unwrap();
    }

    #[test]
    fn new_cards_are_not_mastered() -> Result<()> {
        let mut col = Collection::new();
        col.set_config_bool(BoolKey::Fsrs, true, false)?;
        add_concept_card(&mut col, "concept::biochem::amino_acid_charge");
        let amino = col
            .storage
            .get_concept_id_by_tag("concept::biochem::amino_acid_charge")?
            .unwrap();
        let m = col.concept_mastery(&[amino])?;
        let entry = m.get(&amino).unwrap();
        assert_eq!(entry.total_card_count, 1);
        assert_eq!(entry.scored_card_count, 0);
        assert!(!entry.mastered);
        assert!(entry.has_cards());
        Ok(())
    }

    #[test]
    fn well_remembered_cards_reach_mastery() -> Result<()> {
        let mut col = Collection::new();
        col.set_config_bool(BoolKey::Fsrs, true, false)?;
        let c1 = add_concept_card(&mut col, "concept::biochem::amino_acid_charge");
        let c2 = add_concept_card(&mut col, "concept::biochem::amino_acid_charge");
        let amino = col
            .storage
            .get_concept_id_by_tag("concept::biochem::amino_acid_charge")?
            .unwrap();
        // Only one scored card: below the min-scored-cards floor.
        make_well_remembered(&mut col, c1);
        assert!(!col.concept_is_mastered(amino)?);
        // Two scored, high-retrievability cards: mastered.
        make_well_remembered(&mut col, c2);
        assert!(col.concept_is_mastered(amino)?);
        Ok(())
    }

    #[test]
    fn concept_without_cards_reports_no_cards() -> Result<()> {
        let mut col = Collection::new();
        let orphan = col.storage.get_or_create_concept("concept::test::orphan")?;
        let m = col.concept_mastery(&[orphan])?;
        let entry = m.get(&orphan).unwrap();
        assert_eq!(entry.total_card_count, 0);
        assert!(!entry.has_cards());
        assert!(!entry.mastered);
        Ok(())
    }
}
