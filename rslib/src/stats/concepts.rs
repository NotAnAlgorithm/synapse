// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::HashMap;

use anki_proto::stats::concept_memory_response::ConceptScore;
use anki_proto::stats::ConceptMemoryResponse;
use fsrs::FSRS;
use fsrs::FSRS5_DEFAULT_DECAY;

use crate::prelude::*;
use crate::scheduler::timing::SchedTimingToday;
use crate::search::SortMode;

/// Minimum number of scored cards for a concept's `memory` to be trusted.
const SUFFICIENT_DATA_THRESHOLD: u32 = 3;

#[derive(Default)]
struct ConceptAccumulator {
    /// The concept's section (2nd `::` segment of its tag).
    section: String,
    /// Sum of retrievability (0..1) over cards with an FSRS memory state.
    retrievability_sum: f32,
    /// Total cards mapped to this concept (coverage).
    card_count: u32,
    /// Cards whose memory_state contributed to `retrievability_sum`.
    scored_card_count: u32,
}

impl Collection {
    /// Per-concept "Memory" scores derived from FSRS retrievability, grouped by
    /// the `concept::<section>::<id>` concept layer. Read-model behind the
    /// Synapse Memory dashboard.
    ///
    /// Card->concept membership is read from the derived `card_concepts` /
    /// `concepts` tables (kept in sync with the `concept::` note tags), rather
    /// than scanning each note's tags.
    pub(crate) fn concept_memory(&mut self, search: &str) -> Result<ConceptMemoryResponse> {
        let guard = self.search_cards_into_table(search, SortMode::NoOrder)?;
        guard.col.concept_memory_inner()
    }

    fn concept_memory_inner(&mut self) -> Result<ConceptMemoryResponse> {
        let timing = self.timing_today()?;
        let timing = SchedTimingToday {
            days_elapsed: timing.days_elapsed,
            now: TimestampSecs::now(),
            next_day_at: timing.next_day_at,
        };
        let fsrs = FSRS::new(None)?;

        // card id -> retrievability (only present when the card has an FSRS
        // memory state). Computed once per card.
        let cards = self.storage.all_searched_cards()?;
        let mut retrievability: HashMap<CardId, f32> = HashMap::with_capacity(cards.len());
        for card in &cards {
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
        // mapping. Each (card, concept) pair contributes at most once.
        let mut concepts: HashMap<String, ConceptAccumulator> = HashMap::new();
        for row in self.storage.card_concept_tags_in_search()? {
            let entry = concepts.entry(row.tag).or_default();
            if entry.section.is_empty() {
                entry.section = row.section;
            }
            entry.card_count += 1;
            if let Some(r) = retrievability.get(&row.card_id) {
                entry.retrievability_sum += r;
                entry.scored_card_count += 1;
            }
        }

        let mut scores: Vec<ConceptScore> = concepts
            .into_iter()
            .map(|(concept, acc)| {
                let memory = if acc.scored_card_count > 0 {
                    acc.retrievability_sum * 100.0 / acc.scored_card_count as f32
                } else {
                    0.0
                };
                ConceptScore {
                    concept,
                    section: acc.section,
                    memory,
                    card_count: acc.card_count,
                    scored_card_count: acc.scored_card_count,
                    sufficient_data: acc.scored_card_count >= SUFFICIENT_DATA_THRESHOLD,
                }
            })
            .collect();
        // Deterministic ordering by concept tag.
        scores.sort_by(|a, b| a.concept.cmp(&b.concept));

        Ok(ConceptMemoryResponse { concepts: scores })
    }
}

#[cfg(test)]
mod test {
    use crate::card::FsrsMemoryState;
    use crate::prelude::*;

    /// Give the note's first card an FSRS memory state so it contributes to the
    /// per-concept retrievability average, and return its id.
    fn give_card_memory_state(col: &mut Collection, note_id: NoteId) -> Result<CardId> {
        let cid = col
            .storage
            .all_card_ids_of_note_in_template_order(note_id)?[0];
        let mut card = col.storage.get_card(cid)?.unwrap();
        card.memory_state = Some(FsrsMemoryState {
            stability: 100.0,
            difficulty: 5.0,
        });
        card.last_review_time = Some(TimestampSecs::now());
        col.storage.update_card(&card)?;
        Ok(cid)
    }

    #[test]
    fn concept_memory_aggregates_from_tables() -> Result<()> {
        let mut col = Collection::new();
        let nt = col.get_notetype_by_name("Basic")?.unwrap();

        // note A: one concept, with a fresh memory state (high retrievability)
        let mut a = nt.new_note();
        a.set_field(0, "a")?;
        a.tags = vec!["concept::biochem::amino".into()];
        col.add_note(&mut a, DeckId(1))?;
        give_card_memory_state(&mut col, a.id)?;

        // note B: same concept, no memory state -> counts for coverage only
        let mut b = nt.new_note();
        b.set_field(0, "b")?;
        b.tags = vec!["concept::biochem::amino".into()];
        col.add_note(&mut b, DeckId(1))?;

        // note C: a different concept
        let mut c = nt.new_note();
        c.set_field(0, "c")?;
        c.tags = vec!["concept::physics::kinematics".into()];
        col.add_note(&mut c, DeckId(1))?;

        let resp = col.concept_memory("")?;
        assert_eq!(resp.concepts.len(), 2);

        let amino = &resp.concepts[0];
        assert_eq!(amino.concept, "concept::biochem::amino");
        assert_eq!(amino.section, "biochem");
        assert_eq!(amino.card_count, 2);
        assert_eq!(amino.scored_card_count, 1);
        // one just-reviewed card at stability 100 -> retrievability ~= 100%
        assert!(amino.memory > 99.0, "memory was {}", amino.memory);
        assert!(!amino.sufficient_data);

        let kin = &resp.concepts[1];
        assert_eq!(kin.concept, "concept::physics::kinematics");
        assert_eq!(kin.section, "physics");
        assert_eq!(kin.card_count, 1);
        assert_eq!(kin.scored_card_count, 0);
        assert_eq!(kin.memory, 0.0);

        Ok(())
    }

    #[test]
    fn concept_memory_respects_search_scope() -> Result<()> {
        let mut col = Collection::new();
        let nt = col.get_notetype_by_name("Basic")?.unwrap();

        let mut a = nt.new_note();
        a.set_field(0, "a")?;
        a.tags = vec!["concept::biochem::amino".into()];
        col.add_note(&mut a, DeckId(1))?;

        let mut b = nt.new_note();
        b.set_field(0, "b")?;
        b.tags = vec!["concept::physics::kinematics".into()];
        col.add_note(&mut b, DeckId(1))?;

        // scoping the search to one concept's tag restricts the read model to
        // that concept's cards only.
        let resp = col.concept_memory("tag:concept::biochem::amino")?;
        assert_eq!(resp.concepts.len(), 1);
        assert_eq!(resp.concepts[0].concept, "concept::biochem::amino");
        assert_eq!(resp.concepts[0].card_count, 1);

        // a search matching nothing yields no concepts.
        let resp = col.concept_memory("tag:concept::nope::missing")?;
        assert!(resp.concepts.is_empty());
        Ok(())
    }
}
