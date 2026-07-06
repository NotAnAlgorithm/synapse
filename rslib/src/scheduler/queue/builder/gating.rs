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

/// Notetype-name prefix marking an application-style item.
///
/// This is the same question-type heuristic the interleaving pass uses (see
/// `builder::interleave::is_application_notetype`). It is replicated here as a
/// one-liner rather than shared, to keep that module untouched; both derive
/// from the M1 provisioning convention where application items live under the
/// `MCAT ` notetype namespace. Keep the two in sync.
const APPLICATION_NOTETYPE_PREFIX: &str = "MCAT ";

/// Heuristic: application-style notetypes are those whose name begins with
/// `MCAT `. Everything else (Basic, Cloze, ...) is treated as recall.
fn is_application_notetype(name: &str) -> bool {
    name.starts_with(APPLICATION_NOTETYPE_PREFIX)
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
        // candidates. Notetype names are cached so repeated notetypes cost a
        // single lookup.
        let mut notetype_is_app: HashMap<NotetypeId, bool> = HashMap::new();
        let mut candidate_card_ids: Vec<CardId> = Vec::new();
        for card in &self.new {
            if self.new_card_is_application(col, card.note_id, &mut notetype_is_app)? {
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

    /// Whether the new card on `note_id` uses an application-style notetype.
    fn new_card_is_application(
        &self,
        col: &mut Collection,
        note_id: NoteId,
        notetype_is_app: &mut HashMap<NotetypeId, bool>,
    ) -> Result<bool> {
        // A missing note shouldn't happen for a gathered card; treat it as
        // non-application (never gated) rather than failing the build.
        let Some(note) = col.storage.get_note_without_fields(note_id)? else {
            return Ok(false);
        };
        let notetype_id = note.notetype_id;
        if let Some(is_app) = notetype_is_app.get(&notetype_id) {
            return Ok(*is_app);
        }
        let name = col
            .storage
            .get_notetype(notetype_id)?
            .map(|nt| nt.name)
            .unwrap_or_default();
        let is_app = is_application_notetype(&name);
        notetype_is_app.insert(notetype_id, is_app);
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
    fn application_notetype_heuristic() {
        // Mirrors builder::interleave::is_application_notetype's contract.
        assert!(is_application_notetype("MCAT Application"));
        assert!(is_application_notetype("MCAT Data Snippet"));
        assert!(!is_application_notetype("Basic"));
        assert!(!is_application_notetype("Cloze"));
        assert!(!is_application_notetype("MCAT")); // no trailing space
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

        /// Add one new card tagged `tag`, using `nt`, in `deck`; return its id.
        fn add_tagged_card(&mut self, nt: &Notetype, tag: &str, deck: DeckId) -> CardId {
            let mut note = nt.new_note();
            note.set_field(0, "q").unwrap();
            note.tags = vec![tag.to_string()];
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
    /// `protein_structure` (which depends on the seeded prerequisite
    /// `amino_acids`), and `prereq_cards` recall cards on that
    /// prerequisite. Returns (deck id, prerequisite card ids).
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
            prereq_ids.push(col.add_tagged_card(
                &recall,
                "concept::BB::1A::amino_acids",
                deck.id,
            ));
        }
        col.add_tagged_card(&application, "concept::BB::1A::protein_structure", deck.id);
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
        col.add_tagged_card(&recall, "concept::BB::1A::amino_acids", deck.id);
        col.add_tagged_card(&application, "concept::BB::1A::protein_structure", deck.id);
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
