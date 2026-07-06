// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Synapse curriculum-ordered new-card provisioning.
//!
//! Orders a set of NEW cards so the most basic / highest-impact / least
//! difficult are introduced first, and harder / more dependent material is
//! reached only as the learner builds up. Works alongside the mastery-gating
//! pass (`queue::builder::gating`): gating decides *whether* a new application
//! card may appear yet; this decides the *order* new cards are introduced.
//!
//! Ordering key per card (ascending):
//!   1. prerequisite DEPTH — longest prerequisite chain to a root in the concept
//!      graph (schema 20). Roots (depth 0) first; build the graph up level by level.
//!   2. DIFFICULTY — from the note's `MCAT::difficulty::easy|medium|hard` or
//!      `MCAT::app-difficulty::1|2|3` tag (default medium). Easiest first.
//!   3. IMPACT — out-degree (how many concepts depend on this one), highest first.
//!   4. card id — deterministic tie-break.
//!
//! Two entry points, both writing new-card positions so that with the Synapse
//! preset's `LowestPosition` gather the cards are *introduced* in this order:
//!   - [`Collection::reposition_new_cards_by_curriculum`] — a standalone op over
//!     a deck's new cards (menu / maintenance).
//!   - [`Collection::reposition_cards_by_curriculum_in_place`] — reorders a given
//!     set within the positions they already occupy, called by the text importer
//!     so a deck imported at setup is basic-first with no extra step.

use std::collections::HashMap;

use crate::card::CardType;
use crate::prelude::*;
use crate::scheduler::new::NewCardDueOrder;
use crate::storage::card::NewCardSorting;
use crate::storage::concept::ConceptId;

/// Difficulty used when a card carries no recognizable difficulty tag.
const DEFAULT_DIFFICULTY: u8 = 2;

impl Collection {
    /// Reposition all NEW cards in `deck_id` into curriculum order. Standalone
    /// op (assigns fresh sequential positions via the reposition machinery).
    pub fn reposition_new_cards_by_curriculum(
        &mut self,
        deck_id: DeckId,
    ) -> Result<OpOutput<usize>> {
        let mut card_ids: Vec<CardId> = Vec::new();
        self.storage
            .for_each_new_card_in_deck(deck_id, NewCardSorting::LowestPosition, |card| {
                card_ids.push(card.id);
                Ok(true)
            })?;
        let ordered = self.curriculum_order(&card_ids)?;
        self.sort_cards(&ordered, 1, 1, NewCardDueOrder::Preserve, false)
    }

    /// Reorder `card_ids` (new cards) into curriculum order, reusing the
    /// positions they already occupy so nothing else in the collection shifts.
    /// Writes directly within the caller's transaction (no nested op) — used by
    /// the importer. Returns the number of cards moved.
    pub(crate) fn reposition_cards_by_curriculum_in_place(
        &mut self,
        card_ids: &[CardId],
        usn: Usn,
    ) -> Result<usize> {
        if card_ids.is_empty() {
            return Ok(0);
        }
        let ordered = self.curriculum_order(card_ids)?;
        // Reorder within the lowest position these new cards already hold, so
        // the set occupies its existing range in curriculum order.
        let mut start: Option<u32> = None;
        for cid in card_ids {
            if let Some(card) = self.storage.get_card(*cid)? {
                if card.ctype == CardType::New && card.due >= 0 {
                    let pos = card.due as u32;
                    start = Some(start.map_or(pos, |s| s.min(pos)));
                }
            }
        }
        let Some(start) = start else {
            return Ok(0);
        };
        self.sort_cards_inner(&ordered, start, 1, NewCardDueOrder::Preserve, false, usn)
    }

    /// Order `card_ids` by the curriculum key (prerequisite depth, difficulty,
    /// impact, id). Cards with no concept sort last.
    fn curriculum_order(&self, card_ids: &[CardId]) -> Result<Vec<CardId>> {
        let edges = self.storage.all_concept_edges()?;
        let mut prereqs: HashMap<ConceptId, Vec<ConceptId>> = HashMap::new();
        let mut out_degree: HashMap<ConceptId, u32> = HashMap::new();
        for edge in &edges {
            prereqs.entry(edge.to).or_default().push(edge.from);
            *out_degree.entry(edge.from).or_insert(0) += 1;
        }
        let mut depth_memo: HashMap<ConceptId, u32> = HashMap::new();

        let mut keyed: Vec<(u32, u8, u32, CardId)> = Vec::with_capacity(card_ids.len());
        for cid in card_ids {
            let concept_ids = self.storage.concept_ids_for_card(*cid)?;
            let (depth, impact) = if concept_ids.is_empty() {
                (u32::MAX, 0)
            } else {
                let depth = concept_ids
                    .iter()
                    .map(|c| concept_depth(*c, &prereqs, &mut depth_memo))
                    .min()
                    .unwrap_or(0);
                let impact = concept_ids
                    .iter()
                    .map(|c| out_degree.get(c).copied().unwrap_or(0))
                    .max()
                    .unwrap_or(0);
                (depth, impact)
            };
            let difficulty = self.card_curriculum_difficulty(*cid)?;
            keyed.push((depth, difficulty, u32::MAX - impact, *cid));
        }
        keyed.sort_unstable();
        Ok(keyed.into_iter().map(|(_, _, _, cid)| cid).collect())
    }

    /// Difficulty (1=easy .. 3=hard) for a card, read from its note's tags.
    fn card_curriculum_difficulty(&self, card_id: CardId) -> Result<u8> {
        let Some(card) = self.storage.get_card(card_id)? else {
            return Ok(DEFAULT_DIFFICULTY);
        };
        let Some(note) = self.storage.get_note_without_fields(card.note_id)? else {
            return Ok(DEFAULT_DIFFICULTY);
        };
        for tag in &note.tags {
            if let Some(rest) = tag.strip_prefix("MCAT::difficulty::") {
                return Ok(match rest.to_ascii_lowercase().as_str() {
                    "easy" => 1,
                    "hard" => 3,
                    _ => 2,
                });
            }
            if let Some(rest) = tag.strip_prefix("MCAT::app-difficulty::") {
                return Ok(rest
                    .parse::<u8>()
                    .ok()
                    .filter(|d| (1..=3).contains(d))
                    .unwrap_or(DEFAULT_DIFFICULTY));
            }
        }
        Ok(DEFAULT_DIFFICULTY)
    }
}

/// Longest prerequisite chain ending at `concept` (0 for a root). Memoized;
/// safe on the DAG the prerequisite graph is guaranteed to be.
fn concept_depth(
    concept: ConceptId,
    prereqs: &HashMap<ConceptId, Vec<ConceptId>>,
    memo: &mut HashMap<ConceptId, u32>,
) -> u32 {
    if let Some(d) = memo.get(&concept) {
        return *d;
    }
    memo.insert(concept, 0);
    let depth = match prereqs.get(&concept) {
        None => 0,
        Some(ps) => ps
            .iter()
            .map(|p| concept_depth(*p, prereqs, memo))
            .max()
            .map(|m| m + 1)
            .unwrap_or(0),
    };
    memo.insert(concept, depth);
    depth
}

#[cfg(test)]
mod test {
    use crate::prelude::*;
    use crate::storage::concept::edges::add_test_concept_edge;
    use crate::tests::DeckAdder;

    impl Collection {
        fn add_new_card_with_tags(&mut self, tags: &[&str], deck: DeckId) -> CardId {
            let nt = self.get_notetype_by_name("Basic").unwrap().unwrap();
            let mut note = nt.new_note();
            note.set_field(0, "q").unwrap();
            note.tags = tags.iter().map(|t| t.to_string()).collect();
            self.add_note(&mut note, deck).unwrap();
            self.storage
                .all_card_ids_of_note_in_template_order(note.id)
                .unwrap()[0]
        }
    }

    #[test]
    fn curriculum_orders_foundational_then_easy_then_impact() -> Result<()> {
        let mut col = Collection::new();
        let deck = DeckAdder::new("Synapse").add(&mut col);

        // Graph: root -> dependent, plus a second isolated root (out-degree 0).
        add_test_concept_edge(&col, "concept::BB::1A::root", "concept::BB::1A::dependent");

        let easy_root =
            col.add_new_card_with_tags(&["concept::BB::1A::root", "MCAT::difficulty::easy"], deck.id);
        let hard_root =
            col.add_new_card_with_tags(&["concept::BB::1A::root", "MCAT::difficulty::hard"], deck.id);
        let easy_dep = col.add_new_card_with_tags(
            &["concept::BB::1A::dependent", "MCAT::difficulty::easy"],
            deck.id,
        );
        let iso_root = col.add_new_card_with_tags(
            &["concept::BB::1A::isolated", "MCAT::difficulty::easy"],
            deck.id,
        );

        col.reposition_new_cards_by_curriculum(deck.id)?;
        let pos = |cid: CardId| col.storage.get_card(cid).unwrap().unwrap().due;

        // Depth: root (0) before its dependent (1).
        assert!(pos(easy_root) < pos(easy_dep));
        assert!(pos(hard_root) < pos(easy_dep));
        // Difficulty within the same depth: easy before hard.
        assert!(pos(easy_root) < pos(hard_root));
        // Impact within same depth+difficulty: connected root before isolated root.
        assert!(pos(easy_root) < pos(iso_root));
        Ok(())
    }

    #[test]
    fn in_place_reposition_preserves_position_range() -> Result<()> {
        let mut col = Collection::new();
        let deck = DeckAdder::new("Synapse").add(&mut col);
        add_test_concept_edge(&col, "concept::BB::1A::root", "concept::BB::1A::dependent");
        let dep =
            col.add_new_card_with_tags(&["concept::BB::1A::dependent", "MCAT::difficulty::easy"], deck.id);
        let root =
            col.add_new_card_with_tags(&["concept::BB::1A::root", "MCAT::difficulty::easy"], deck.id);
        let ids = [dep, root];
        let before: Vec<i32> = ids
            .iter()
            .map(|c| col.storage.get_card(*c).unwrap().unwrap().due)
            .collect();
        let usn = col.usn()?;
        col.reposition_cards_by_curriculum_in_place(&ids, usn)?;
        let pos = |c: CardId| col.storage.get_card(c).unwrap().unwrap().due;
        // Root (depth 0) now precedes its dependent...
        assert!(pos(root) < pos(dep));
        // ...and the set still occupies exactly the same two positions.
        let mut after = [pos(dep), pos(root)];
        after.sort_unstable();
        let mut sorted_before = before.clone();
        sorted_before.sort_unstable();
        assert_eq!(after.to_vec(), sorted_before);
        Ok(())
    }
}
