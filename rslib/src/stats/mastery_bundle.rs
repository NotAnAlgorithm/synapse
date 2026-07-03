// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! C2 "concept + weak prerequisites" mastery bundle (tutor read-model).
//!
//! When a student misses an item, the tutor needs the missed concept's mastery
//! signal PLUS the same signal for its DIRECT prerequisites, ordered
//! weakest-first, so it can name the specific prerequisite that is actually
//! holding the learner back (PRD C2; `notes/M3_tutor_design.md` §2). This
//! module is a pure rollup — it invents no new data. It composes:
//!
//! - [`SqliteStorage::get_concept_id_by_tag`] — resolve a requested tag to its
//!   local concept id;
//! - [`SqliteStorage::get_prerequisites`] — the concept's DIRECT prerequisites
//!   (one hop; `concept_edges` rows where the focus is the dependent `to`);
//! - [`SqliteStorage::all_concepts`] — resolve prerequisite ids back to tags;
//! - the same FSRS-retrievability aggregation the Memory dashboard uses
//!   ([`Collection::concept_memory`], `stats/concepts.rs`).
//!
//! ## Why the mastery signal is recomputed here rather than read from
//! ## `concept_mastery`
//!
//! The design (`§2.2`) requires the bundle's numbers to be CONSISTENT with
//! [`Collection::concept_memory`] for the *same* `search` scope, so the tutor,
//! the Memory dashboard and the concept graph never disagree. But
//! [`Collection::concept_mastery`] (`storage/concept/mastery.rs`) reads a
//! concept's cards through `card_ids_for_concept`, which scans the WHOLE
//! `card_concepts` table and ignores the `search_cids` scope — its counts would
//! diverge from `concept_memory`'s under a non-empty `search`. So, exactly like
//! the sibling Performance read-model (`stats/performance.rs`), we recompute
//! the per-concept retrievability aggregate over the SEARCHED cards
//! (`card_concept_tags_in_search`), which is precisely what `concept_memory`
//! aggregates. `card_count`, `scored_card_count` and `memory` are therefore
//! identical to what `concept_memory` reports for the scope; `has_cards` is
//! `card_count > 0` *within that scope* (a concept with cards only outside the
//! scope reads as "nothing to study here" for this view).
//!
//! The `mastered` flag then applies the exact same decision rule
//! `concept_mastery` uses (`>= MASTERY_MIN_SCORED_CARDS` scored cards clearing
//! `MASTERY_RETRIEVABILITY_THRESHOLD` mean retrievability). Those thresholds
//! live in a private storage submodule and cannot be imported from here, so —
//! like every other read-model that carries a threshold (`concepts.rs`,
//! `performance.rs`, `mastery.rs` each define their own
//! `SUFFICIENT_DATA_THRESHOLD`) — they are mirrored below and kept in lockstep.

use std::collections::HashMap;

use anki_proto::stats::concept_mastery_bundle_response::Bundle;
use anki_proto::stats::concept_mastery_bundle_response::ConceptState;
use anki_proto::stats::ConceptMasteryBundleResponse;
use fsrs::FSRS;
use fsrs::FSRS5_DEFAULT_DECAY;

use crate::prelude::*;
use crate::scheduler::timing::SchedTimingToday;
use crate::search::SortMode;

/// Minimum number of scored cards for a concept's `memory` to be trusted.
/// Mirrors `SUFFICIENT_DATA_THRESHOLD` in `stats/concepts.rs` so the tutor's
/// abstention gate agrees with the Memory dashboard's.
const SUFFICIENT_DATA_THRESHOLD: u32 = 3;

/// Minimum mean retrievability (0..1) for a concept to count as mastered.
/// Mirrors `MASTERY_RETRIEVABILITY_THRESHOLD` in `storage/concept/mastery.rs`
/// (private to `storage`, so it cannot be imported here) — keep in lockstep so
/// `mastered` agrees with the mastery/gating signal the scheduler uses.
const MASTERY_RETRIEVABILITY_THRESHOLD: f32 = 0.9;
/// Minimum scored cards before `mastered` is trusted. Mirrors
/// `MASTERY_MIN_SCORED_CARDS` in `storage/concept/mastery.rs`.
const MASTERY_MIN_SCORED_CARDS: u32 = 2;

#[derive(Default, Clone)]
struct ConceptAccumulator {
    /// The concept's section (2nd `::` segment of its tag).
    section: String,
    /// Sum of retrievability (0..1) over cards with an FSRS memory state.
    retrievability_sum: f32,
    /// Total cards mapped to this concept within the searched scope (coverage).
    card_count: u32,
    /// Cards whose memory_state contributed to `retrievability_sum`.
    scored_card_count: u32,
}

impl ConceptAccumulator {
    /// Project the accumulator into a [`ConceptState`] for `concept`. Applies
    /// the same `memory`/`sufficient_data` math as `concept_memory` and the
    /// same `mastered` rule as `concept_mastery`, so all three views agree.
    fn into_state(self, concept: String) -> ConceptState {
        let mean = if self.scored_card_count > 0 {
            self.retrievability_sum / self.scored_card_count as f32
        } else {
            0.0
        };
        let mastered = self.scored_card_count >= MASTERY_MIN_SCORED_CARDS
            && mean >= MASTERY_RETRIEVABILITY_THRESHOLD;
        ConceptState {
            concept,
            section: self.section,
            memory: mean * 100.0,
            card_count: self.card_count,
            scored_card_count: self.scored_card_count,
            sufficient_data: self.scored_card_count >= SUFFICIENT_DATA_THRESHOLD,
            mastered,
            has_cards: self.card_count > 0,
        }
    }
}

impl Collection {
    /// The C2 mastery bundle (PRD C2): for each requested concept tag, its own
    /// Memory/mastery signal plus the same signal for its DIRECT prerequisites,
    /// ordered weakest-first. `search` scopes the card population exactly like
    /// [`Collection::concept_memory`] (empty = whole collection), so the
    /// tutor's numbers match the Memory dashboard's for the same scope.
    ///
    /// An unknown/untagged concept yields a bundle whose `focus` has no cards
    /// (zeros, `has_cards = false`) and no prerequisites — it is never an
    /// error, so a miss on an untagged item degrades gracefully.
    pub(crate) fn concept_mastery_bundle(
        &mut self,
        concepts: &[String],
        search: &str,
    ) -> Result<ConceptMasteryBundleResponse> {
        let guard = self.search_cards_into_table(search, SortMode::NoOrder)?;
        guard.col.concept_mastery_bundle_inner(concepts)
    }

    fn concept_mastery_bundle_inner(
        &mut self,
        concepts: &[String],
    ) -> Result<ConceptMasteryBundleResponse> {
        // Per-concept retrievability aggregate over the SEARCHED cards — the
        // exact aggregation `concept_memory` performs, so the bundle's numbers
        // are consistent with the dashboard for this scope.
        let accumulators = self.concept_accumulators_in_search()?;

        // Resolve concept ids -> tags once so prerequisite ids can be rendered
        // back to tags without re-querying per concept (mirrors graph.rs /
        // performance.rs). Keyed by the raw i64 id so this module doesn't need
        // to name storage's private `ConceptId` newtype.
        let id_to_tag: HashMap<i64, String> = self
            .storage
            .all_concepts()?
            .into_iter()
            .map(|c| (c.id.0, c.tag))
            .collect();

        let mut bundles = Vec::with_capacity(concepts.len());
        for concept in concepts {
            bundles.push(self.build_bundle(concept, &accumulators, &id_to_tag)?);
        }
        Ok(ConceptMasteryBundleResponse { bundles })
    }

    /// Assemble one focus concept's bundle: its own [`ConceptState`] plus its
    /// direct prerequisites, weakest-first.
    fn build_bundle(
        &self,
        concept: &str,
        accumulators: &HashMap<String, ConceptAccumulator>,
        id_to_tag: &HashMap<i64, String>,
    ) -> Result<Bundle> {
        let focus = state_for_tag(concept, accumulators);

        // Direct prerequisites (one hop). Unknown tag -> no prerequisites (and a
        // no-card focus above), never an error.
        let prerequisites = match self.storage.get_concept_id_by_tag(concept)? {
            Some(id) => {
                let mut prereqs: Vec<ConceptState> = self
                    .storage
                    .get_prerequisites(id)?
                    .into_iter()
                    .filter_map(|prereq| id_to_tag.get(&prereq.0))
                    .map(|tag| state_for_tag(tag, accumulators))
                    .collect();
                sort_weakest_first(&mut prereqs);
                prereqs
            }
            None => Vec::new(),
        };

        Ok(Bundle {
            focus: Some(focus),
            prerequisites,
        })
    }

    /// Per-full-tag retrievability accumulators over the cards currently in the
    /// `search_cids` table. Each (card, concept) pair contributes at most once,
    /// exactly as `concept_memory_inner` accumulates.
    fn concept_accumulators_in_search(&mut self) -> Result<HashMap<String, ConceptAccumulator>> {
        let timing = self.timing_today()?;
        let timing = SchedTimingToday {
            days_elapsed: timing.days_elapsed,
            now: TimestampSecs::now(),
            next_day_at: timing.next_day_at,
        };
        let fsrs = FSRS::new(None)?;

        // card id -> retrievability (only present when the card has an FSRS
        // memory state). Computed once per card, like `concepts.rs`.
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
        Ok(concepts)
    }
}

/// The [`ConceptState`] for a full concept tag: read its accumulator if the
/// concept has cards in scope, else a zeroed "no cards" state (`has_cards =
/// false`) so an unknown/out-of-scope concept is representable, not an error.
fn state_for_tag(tag: &str, accumulators: &HashMap<String, ConceptAccumulator>) -> ConceptState {
    match accumulators.get(tag) {
        // Clone the (small) accumulator so `into_state` can consume it; this
        // keeps `state_for_tag` a pure read that can be called for both the
        // focus and each prerequisite of a bundle.
        Some(acc) => acc.clone().into_state(tag.to_string()),
        None => ConceptState {
            concept: tag.to_string(),
            section: section_of_tag(tag).to_string(),
            memory: 0.0,
            card_count: 0,
            scored_card_count: 0,
            sufficient_data: false,
            mastered: false,
            has_cards: false,
        },
    }
}

/// Extract the `<section>` (2nd `::` segment) of a concept tag, "" if absent.
/// Local copy of `storage::concept::section_of_concept_tag` (private to
/// `storage`) so a concept with no cards in scope still carries its section.
fn section_of_tag(tag: &str) -> &str {
    tag.split("::").nth(1).unwrap_or_default()
}

/// Order prerequisites weakest-first so `prerequisites[0]` is the tutor's
/// headline "the thing actually holding you back": unmastered-with-cards before
/// everything else, then ascending `memory` within each group. Ties broken by
/// tag for determinism.
fn sort_weakest_first(prereqs: &mut [ConceptState]) {
    prereqs.sort_by(|a, b| {
        // A concept is a "weak prerequisite" candidate only when it has cards
        // and is not mastered; those come first.
        let a_weak = a.has_cards && !a.mastered;
        let b_weak = b.has_cards && !b.mastered;
        b_weak
            .cmp(&a_weak)
            .then(
                a.memory
                    .partial_cmp(&b.memory)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then_with(|| a.concept.cmp(&b.concept))
    });
}

#[cfg(test)]
mod test {
    use crate::card::FsrsMemoryState;
    use crate::prelude::*;

    /// Add a `Basic` note tagged with `tag`, returning its single card id.
    fn add_concept_card(col: &mut Collection, front: &str, tag: &str) -> CardId {
        let nt = col.get_notetype_by_name("Basic").unwrap().unwrap();
        let mut note = nt.new_note();
        note.set_field(0, front).unwrap();
        note.tags = vec![tag.to_string()];
        col.add_note(&mut note, DeckId(1)).unwrap();
        col.storage
            .all_card_ids_of_note_in_template_order(note.id)
            .unwrap()[0]
    }

    /// Force a card into a strongly-remembered review state so its
    /// retrievability is ~1.0 (mirrors the mastery.rs test helper).
    fn make_well_remembered(col: &mut Collection, cid: CardId) {
        let mut card = col.storage.get_card(cid).unwrap().unwrap();
        card.memory_state = Some(FsrsMemoryState {
            stability: 100.0,
            difficulty: 5.0,
        });
        card.last_review_time = Some(TimestampSecs::now());
        col.storage.update_card(&card).unwrap();
    }

    #[test]
    fn focus_with_prerequisites_orders_weakest_first() -> Result<()> {
        let mut col = Collection::new();
        // enzyme_kinetics depends (via SEED_EDGES) on amino_acid_charge and
        // protein_structure.
        add_concept_card(&mut col, "kinetics", "concept::biochem::enzyme_kinetics");

        // amino_acid_charge: two well-remembered scored cards -> mastered.
        let a1 = add_concept_card(&mut col, "a1", "concept::biochem::amino_acid_charge");
        let a2 = add_concept_card(&mut col, "a2", "concept::biochem::amino_acid_charge");
        make_well_remembered(&mut col, a1);
        make_well_remembered(&mut col, a2);

        // protein_structure: has a card but no memory state -> unmastered,
        // has_cards, memory 0 -> it is the weak prerequisite.
        add_concept_card(&mut col, "ps", "concept::biochem::protein_structure");

        let resp =
            col.concept_mastery_bundle(&["concept::biochem::enzyme_kinetics".to_string()], "")?;
        assert_eq!(resp.bundles.len(), 1);
        let bundle = &resp.bundles[0];

        let focus = bundle.focus.as_ref().expect("focus present");
        assert_eq!(focus.concept, "concept::biochem::enzyme_kinetics");
        assert_eq!(focus.section, "biochem");
        assert!(focus.has_cards);
        assert_eq!(focus.card_count, 1);

        // Both seed prerequisites are present.
        assert_eq!(bundle.prerequisites.len(), 2);
        let tags: Vec<&str> = bundle
            .prerequisites
            .iter()
            .map(|p| p.concept.as_str())
            .collect();
        assert!(tags.contains(&"concept::biochem::amino_acid_charge"));
        assert!(tags.contains(&"concept::biochem::protein_structure"));

        // Weakest-first: the unmastered-with-cards prerequisite leads.
        let top = &bundle.prerequisites[0];
        assert_eq!(top.concept, "concept::biochem::protein_structure");
        assert!(top.has_cards);
        assert!(!top.mastered);
        assert_eq!(top.memory, 0.0);

        // The mastered prerequisite comes last and reads as mastered.
        let amino = &bundle.prerequisites[1];
        assert_eq!(amino.concept, "concept::biochem::amino_acid_charge");
        assert!(amino.mastered);
        assert!(amino.has_cards);
        assert_eq!(amino.scored_card_count, 2);
        assert!(amino.memory > 99.0, "memory was {}", amino.memory);

        Ok(())
    }

    #[test]
    fn unknown_tag_yields_empty_focus_without_erroring() -> Result<()> {
        let mut col = Collection::new();
        let resp = col.concept_mastery_bundle(&["concept::nope::missing".to_string()], "")?;
        assert_eq!(resp.bundles.len(), 1);
        let bundle = &resp.bundles[0];
        let focus = bundle.focus.as_ref().expect("focus present");
        assert_eq!(focus.concept, "concept::nope::missing");
        // Section is still parsed from the tag even with no cards.
        assert_eq!(focus.section, "nope");
        assert!(!focus.has_cards);
        assert_eq!(focus.card_count, 0);
        assert_eq!(focus.scored_card_count, 0);
        assert_eq!(focus.memory, 0.0);
        assert!(!focus.mastered);
        // No cards, no concept id -> no prerequisites, no error.
        assert!(bundle.prerequisites.is_empty());
        Ok(())
    }

    #[test]
    fn search_scopes_the_rollup() -> Result<()> {
        let mut col = Collection::new();
        // Two cards for the focus concept, one for a prerequisite. Scoping to a
        // single card's note restricts the counts to what is in scope.
        add_concept_card(&mut col, "k1", "concept::biochem::enzyme_kinetics");
        add_concept_card(&mut col, "k2", "concept::biochem::enzyme_kinetics");
        add_concept_card(&mut col, "amino", "concept::biochem::amino_acid_charge");

        // Whole collection: focus sees both its cards; the seeded prerequisite
        // that has a card in scope reads has_cards.
        let full =
            col.concept_mastery_bundle(&["concept::biochem::enzyme_kinetics".to_string()], "")?;
        let full_focus = full.bundles[0].focus.as_ref().unwrap();
        assert_eq!(full_focus.card_count, 2);
        let amino_full = full.bundles[0]
            .prerequisites
            .iter()
            .find(|p| p.concept == "concept::biochem::amino_acid_charge")
            .expect("amino prerequisite present");
        assert!(amino_full.has_cards);

        // Scope to just the focus concept's cards. The focus is unchanged, but
        // the out-of-scope prerequisite now reads "no cards in scope".
        let scoped = col.concept_mastery_bundle(
            &["concept::biochem::enzyme_kinetics".to_string()],
            "tag:concept::biochem::enzyme_kinetics",
        )?;
        let scoped_focus = scoped.bundles[0].focus.as_ref().unwrap();
        assert_eq!(scoped_focus.card_count, 2);
        let amino_scoped = scoped.bundles[0]
            .prerequisites
            .iter()
            .find(|p| p.concept == "concept::biochem::amino_acid_charge")
            .expect("amino prerequisite still listed");
        assert!(!amino_scoped.has_cards);
        assert_eq!(amino_scoped.card_count, 0);

        // A search matching nothing zeroes the focus but never errors.
        let empty = col.concept_mastery_bundle(
            &["concept::biochem::enzyme_kinetics".to_string()],
            "tag:concept::nope::missing",
        )?;
        let empty_focus = empty.bundles[0].focus.as_ref().unwrap();
        assert!(!empty_focus.has_cards);
        assert_eq!(empty_focus.card_count, 0);
        Ok(())
    }
}
