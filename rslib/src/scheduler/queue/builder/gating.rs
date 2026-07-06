// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Synapse prerequisite-graph mastery gating for the v3 queue builder.
//!
//! When the deck-config toggle `mastery_gating` is enabled, the builder
//! withholds NEW *application*-type cards whose concept still has an unmastered
//! prerequisite (per the concept graph, schema 20). The intent: don't introduce
//! "apply the skill" items before the underlying recall foundations are solid.
//!
//! Scope is deliberately narrow and conservative:
//! - only **NEW** cards are ever withheld — cards already in learning/review
//!   have been started and are never held back;
//! - only **application**-type cards are considered — recall cards (the
//!   foundations themselves) always flow through;
//! - a withheld card is simply left out of *this* build; nothing is mutated, so
//!   it reappears automatically once its prerequisites are mastered.
//!
//! This runs as a filtering pass over `self.new` after gathering, mirroring how
//! the interleaving pass hooks in. When the toggle is off, none of this code
//! runs and behaviour is byte-for-byte unchanged.

use std::collections::HashMap;

use super::QueueBuilder;
use crate::prelude::*;

/// Tags marking a card as an application/practice item — the kind gated behind
/// prerequisite mastery. Recall cards (which carry `MCAT::difficulty::*`, or no
/// difficulty tag) are never gated and always flow through.
///
/// Classifying by TAG rather than notetype is deliberate: recall *and*
/// application content share the "MCAT Application" notetype in the provisioned
/// demo, so the notetype name cannot tell them apart. The generation pipeline
/// tags application questions with `MCAT::practice` + `MCAT::app-difficulty::N`;
/// recall cards get `MCAT::difficulty::*`. (The interleave pass still uses the
/// coarser notetype-name heuristic — that only affects cosmetic spreading, not
/// what is withheld.)
const APPLICATION_PRACTICE_TAG: &str = "MCAT::practice";
const APPLICATION_DIFFICULTY_PREFIX: &str = "MCAT::app-difficulty::";

/// Whether a note's tags mark it as an application/practice item.
fn note_is_application(tags: &[String]) -> bool {
    tags.iter()
        .any(|t| t == APPLICATION_PRACTICE_TAG || t.starts_with(APPLICATION_DIFFICULTY_PREFIX))
}

impl QueueBuilder {
    /// Remove NEW application-type cards whose concept has an unmastered
    /// prerequisite from the gathered new-card pool. Called only when the
    /// `mastery_gating` toggle is on. A no-op (and zero extra queries) when no
    /// gathered new card is an application item.
    pub(super) fn apply_mastery_gating(&mut self, col: &mut Collection) -> Result<()> {
        if self.new.is_empty() {
            return Ok(());
        }

        // Identify the application-type new cards; only these are gate
        // candidates. Classifications are cached by note id so sibling cards
        // (which share a note) cost a single lookup.
        let mut note_is_app: HashMap<NoteId, bool> = HashMap::new();
        let mut candidate_card_ids: Vec<CardId> = Vec::new();
        for card in &self.new {
            if self.new_card_is_application(col, card.note_id, &mut note_is_app)? {
                candidate_card_ids.push(card.id);
            }
        }
        if candidate_card_ids.is_empty() {
            return Ok(());
        }

        let withheld = col.concept_gated_card_ids(&candidate_card_ids)?;
        if withheld.is_empty() {
            return Ok(());
        }

        // Drop withheld cards; counts in `build()` are derived from `self.new`,
        // so they stay correct automatically.
        self.new.retain(|card| !withheld.contains(&card.id));
        Ok(())
    }

    /// Whether the new card on `note_id` is an application/practice item, per
    /// its tags. Cached by note id.
    fn new_card_is_application(
        &self,
        col: &mut Collection,
        note_id: NoteId,
        note_is_app: &mut HashMap<NoteId, bool>,
    ) -> Result<bool> {
        if let Some(is_app) = note_is_app.get(&note_id) {
            return Ok(*is_app);
        }
        // A missing note shouldn't happen for a gathered card; treat it as
        // non-application (never gated) rather than failing the build.
        let is_app = match col.storage.get_note_without_fields(note_id)? {
            Some(note) => note_is_application(&note.tags),
            None => false,
        };
        note_is_app.insert(note_id, is_app);
        Ok(is_app)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::card::CardQueue;
    use crate::card::CardType;
    use crate::card::FsrsMemoryState;
    use crate::deckconfig::DeckConfig;
    use crate::notetype::all_stock_notetypes;
    use crate::tests::DeckAdder;

    #[test]
    fn application_tag_heuristic() {
        // Application items are classified by tag, not notetype.
        assert!(note_is_application(&["MCAT::practice".to_string()]));
        assert!(note_is_application(&[
            "concept::BB::1A::x".to_string(),
            "MCAT::app-difficulty::2".to_string(),
        ]));
        // Recall cards (a difficulty tag, or none) are not application.
        assert!(!note_is_application(&["MCAT::difficulty::easy".to_string()]));
        assert!(!note_is_application(&["concept::BB::1A::x".to_string()]));
        assert!(!note_is_application(&[]));
    }

    impl Collection {
        /// Add a stock-Basic-derived notetype under `name` (so a "MCAT " prefix
        /// classifies it as application).
        fn add_gating_notetype(&mut self, name: &str) -> Notetype {
            let mut nt = all_stock_notetypes(&self.tr).remove(0);
            nt.name = name.into();
            self.add_notetype(&mut nt, false).unwrap();
            nt
        }

        /// Add one new card with `tags`, using `nt`, in `deck`; return its id.
        fn add_tagged_card(&mut self, nt: &Notetype, tags: &[&str], deck: DeckId) -> CardId {
            let mut note = nt.new_note();
            note.set_field(0, "q").unwrap();
            note.tags = tags.iter().map(|t| t.to_string()).collect();
            self.add_note(&mut note, deck).unwrap();
            self.storage
                .all_card_ids_of_note_in_template_order(note.id)
                .unwrap()[0]
        }

        /// Force a card into a strongly-remembered review state (retrievability
        /// ~1.0), so its concept can clear the mastery threshold.
        fn make_well_remembered(&mut self, cid: CardId) {
            let orig = self.storage.get_card(cid).unwrap().unwrap();
            let mut card = orig.clone();
            card.ctype = CardType::Review;
            card.queue = CardQueue::Review;
            card.interval = 60;
            card.due = 0;
            card.memory_state = Some(FsrsMemoryState {
                stability: 200.0,
                difficulty: 5.0,
            });
            card.last_review_time = Some(TimestampSecs::now());
            self.update_card_inner(&mut card, orig, self.usn().unwrap())
                .unwrap();
        }

        /// How many cards in `deck`'s built queue belong to the application
        /// notetype ("MCAT " prefix).
        fn queue_application_card_count(&mut self, deck: DeckId) -> usize {
            self.build_queues(deck)
                .unwrap()
                .iter()
                .filter(|entry| {
                    let card = self.storage.get_card(entry.card_id()).unwrap().unwrap();
                    let note = self.storage.get_note(card.note_id).unwrap().unwrap();
                    let nt = self
                        .storage
                        .get_notetype(note.notetype_id)
                        .unwrap()
                        .unwrap();
                    nt.name.starts_with("MCAT ")
                })
                .count()
        }
    }

    /// Build a deck with mastery gating on, an application card on
    /// `protein_structure` (which depends on the prerequisite `amino_acids`),
    /// and `prereq_cards` recall cards on that prerequisite. The prerequisite
    /// edge is built in-test (the production spine seed is now empty). Returns
    /// (deck id, prerequisite card ids).
    fn setup_gated_deck(col: &mut Collection, prereq_cards: usize) -> (DeckId, Vec<CardId>) {
        let deck = DeckAdder::new("Synapse")
            .with_config(|c: &mut DeckConfig| {
                c.inner.mastery_gating = true;
            })
            .add(col);

        let recall = col.add_gating_notetype("Basic Recall");
        let application = col.add_gating_notetype("MCAT Application");

        let mut prereq_ids = Vec::new();
        for _ in 0..prereq_cards {
            prereq_ids.push(col.add_tagged_card(&recall, &["concept::BB::1A::amino_acids"], deck.id));
        }
        col.add_tagged_card(
            &application,
            &["concept::BB::1A::protein_structure", "MCAT::app-difficulty::2"],
            deck.id,
        );
        crate::storage::concept::edges::add_test_concept_edge(
            col,
            "concept::BB::1A::amino_acids",
            "concept::BB::1A::protein_structure",
        );
        (deck.id, prereq_ids)
    }

    #[test]
    fn gating_withholds_then_admits_application_card() -> Result<()> {
        let mut col = Collection::new();
        let (deck, prereq_ids) = setup_gated_deck(&mut col, 2);

        // Prerequisite is brand-new (unmastered): the application card is held
        // back.
        assert_eq!(col.queue_application_card_count(deck), 0);

        // Master the prerequisite: its recall cards become well-remembered
        // reviews, clearing the mastery threshold.
        for cid in &prereq_ids {
            col.make_well_remembered(*cid);
        }

        // Now the application card is admitted.
        assert_eq!(col.queue_application_card_count(deck), 1);
        Ok(())
    }

    #[test]
    fn gating_off_admits_application_card_regardless() -> Result<()> {
        let mut col = Collection::new();
        let deck = DeckAdder::new("Synapse")
            .with_config(|c: &mut DeckConfig| {
                c.inner.mastery_gating = false;
            })
            .add(&mut col);
        let recall = col.add_gating_notetype("Basic Recall");
        let application = col.add_gating_notetype("MCAT Application");
        // Unmastered prerequisite present...
        col.add_tagged_card(&recall, &["concept::BB::1A::amino_acids"], deck.id);
        col.add_tagged_card(
            &application,
            &["concept::BB::1A::protein_structure", "MCAT::app-difficulty::2"],
            deck.id,
        );
        // ...but with gating off the application card still appears.
        assert_eq!(col.queue_application_card_count(deck.id), 1);
        Ok(())
    }

    #[test]
    fn gating_never_withholds_recall_cards() -> Result<()> {
        let mut col = Collection::new();
        let (deck, _) = setup_gated_deck(&mut col, 2);
        // Even with the application card gated, the two recall prerequisite
        // cards flow through (recall cards are never withheld).
        let recall_count = col
            .build_queues(deck)?
            .iter()
            .filter(|entry| {
                let card = col.storage.get_card(entry.card_id()).unwrap().unwrap();
                let note = col.storage.get_note(card.note_id).unwrap().unwrap();
                let nt = col.storage.get_notetype(note.notetype_id).unwrap().unwrap();
                !nt.name.starts_with("MCAT ")
            })
            .count();
        assert_eq!(recall_count, 2);
        Ok(())
    }
}
