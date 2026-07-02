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

/// Note-tag prefix identifying a concept tag, e.g.
/// `concept::biochem::amino_acid_charge`.
const CONCEPT_TAG_PREFIX: &str = "concept::";
/// Minimum number of scored cards for a concept's `memory` to be trusted.
const SUFFICIENT_DATA_THRESHOLD: u32 = 3;

#[derive(Default)]
struct ConceptAccumulator {
    /// Sum of retrievability (0..1) over cards with an FSRS memory state.
    retrievability_sum: f32,
    /// Total cards mapped to this concept (coverage).
    card_count: u32,
    /// Cards whose memory_state contributed to `retrievability_sum`.
    scored_card_count: u32,
}

impl Collection {
    /// Per-concept "Memory" scores derived from FSRS retrievability, grouped by
    /// the `concept::<section>::<id>` note-tag convention. Read-model behind
    /// the Synapse Memory dashboard.
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

        let cards = self.storage.all_searched_cards()?;
        // Cache note tags so multiple cards on the same note only trigger one
        // lookup.
        // TODO(synapse): batch note-tag lookup rather than one query per note.
        let mut note_tags: HashMap<NoteId, Vec<String>> = HashMap::new();
        // full concept tag -> accumulator
        let mut concepts: HashMap<String, ConceptAccumulator> = HashMap::new();

        for card in cards {
            let tags = match note_tags.entry(card.note_id) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    let tags = self
                        .storage
                        .get_note_without_fields(card.note_id)?
                        .map(|note| note.tags)
                        .unwrap_or_default();
                    e.insert(tags)
                }
            };

            // A card contributes at most once per distinct concept tag on its
            // note.
            let retrievability = card.memory_state.map(|state| {
                let elapsed_seconds = card.seconds_since_last_review(&timing).unwrap_or_default();
                fsrs.current_retrievability_seconds(
                    state.into(),
                    elapsed_seconds,
                    card.decay.unwrap_or(FSRS5_DEFAULT_DECAY),
                )
            });

            for tag in tags {
                if !tag.starts_with(CONCEPT_TAG_PREFIX) {
                    continue;
                }
                let entry = concepts.entry(tag.clone()).or_default();
                entry.card_count += 1;
                if let Some(r) = retrievability {
                    entry.retrievability_sum += r;
                    entry.scored_card_count += 1;
                }
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
                let section = concept.split("::").nth(1).unwrap_or_default().to_string();
                ConceptScore {
                    concept,
                    section,
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
