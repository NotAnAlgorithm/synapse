// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Trickle-down credit across the prerequisite graph.
//!
//! When a learner successfully answers an *application* item, that success also
//! reflects (partial) command of the concept's PREREQUISITES — applying a skill
//! exercises the foundations it builds on. Rather than leave those prerequisite
//! cards to come due on their own schedule, we grant them a small, discounted
//! reinforcement: a modest bump to FSRS stability with a matching stretch of
//! the interval/due date, so they are surfaced slightly less often.
//!
//! This is deliberately conservative and reversible-in-spirit (it only ever
//! *strengthens* a card, never weakens or hides it) and is gated behind the
//! `trickle_down_credit` deck-config flag, which defaults off. Constants are
//! tunable.
//!
//! The heavy lifting lives here (in the concept storage layer, which owns the
//! graph + card/concept mapping); `scheduler::answering` calls a thin helper
//! that checks the flag and rating before delegating.

use std::collections::HashSet;

use super::ConceptId;
use crate::card::Card;
use crate::card::CardQueue;
use crate::prelude::*;

/// Multiplicative bump applied to a prerequisite review card's FSRS stability
/// (and, proportionally, its interval) on a downstream application success. A
/// 5% nudge is intentionally small — trickle-down is a gentle assist, not a
/// full review. Tunable.
const TRICKLE_DOWN_STABILITY_FACTOR: f32 = 1.05;
/// Hard ceiling (days) on a card's interval after a trickle-down stretch, so
/// the nudge can never balloon an interval. Matches the stock default maximum
/// review interval. Tunable.
const TRICKLE_DOWN_MAX_INTERVAL: u32 = 36_500;

impl Collection {
    /// Grant discounted trickle-down credit to the prerequisite concepts of the
    /// concepts on `answered_card_id`. Intended to be called after an
    /// *application* card is answered *successfully*; the caller owns those two
    /// conditions (see `scheduler::answering`).
    ///
    /// For every prerequisite concept of every concept on the answered card,
    /// any mapped card currently in the review queue with an FSRS memory
    /// state has its stability (and interval/due) nudged upward by
    /// [`TRICKLE_DOWN_STABILITY_FACTOR`]. New/learning/suspended/buried cards
    /// are left untouched — trickle-down only reinforces already-scheduled
    /// reviews. Runs within the caller's transaction.
    pub(crate) fn apply_trickle_down_credit(&mut self, answered_card_id: CardId) -> Result<()> {
        let concepts = self.storage.concept_ids_for_card(answered_card_id)?;
        if concepts.is_empty() {
            return Ok(());
        }

        // Union of prerequisite concepts across the answered card's concepts.
        let mut prereqs: Vec<ConceptId> = Vec::new();
        for concept in concepts {
            prereqs.extend(self.storage.get_prerequisites(concept)?);
        }
        prereqs.sort_unstable();
        prereqs.dedup();
        if prereqs.is_empty() {
            return Ok(());
        }

        // Collect the distinct prerequisite cards. A card can map to several
        // prerequisite concepts; credit it at most once.
        let mut seen: HashSet<CardId> = HashSet::new();
        let usn = self.usn()?;
        for prereq in prereqs {
            for cid in self.storage.card_ids_for_concept(prereq)? {
                if !seen.insert(cid) {
                    continue;
                }
                let Some(card) = self.storage.get_card(cid)? else {
                    continue;
                };
                if let Some(updated) = trickle_down_updated_card(&card) {
                    let mut updated = updated;
                    self.update_card_inner(&mut updated, card, usn)?;
                }
            }
        }
        Ok(())
    }
}

/// Return a strengthened copy of `card` if it is an eligible review card,
/// otherwise `None`. Eligibility: sitting in the review queue with an FSRS
/// memory state. The stability and interval are scaled by
/// [`TRICKLE_DOWN_STABILITY_FACTOR`] (interval clamped to
/// [`TRICKLE_DOWN_MAX_INTERVAL`]), and the due date is pushed out by the
/// interval delta so the card comes up correspondingly later.
fn trickle_down_updated_card(card: &Card) -> Option<Card> {
    if card.queue != CardQueue::Review {
        return None;
    }
    let memory = card.memory_state?;

    let mut updated = card.clone();
    updated.memory_state = Some(crate::card::FsrsMemoryState {
        stability: memory.stability * TRICKLE_DOWN_STABILITY_FACTOR,
        difficulty: memory.difficulty,
    });

    let old_interval = card.interval;
    let new_interval = ((old_interval as f32 * TRICKLE_DOWN_STABILITY_FACTOR).round() as u32)
        .max(old_interval)
        .min(TRICKLE_DOWN_MAX_INTERVAL);
    let delta = new_interval.saturating_sub(old_interval);
    updated.interval = new_interval;
    // Review cards store `due` as a day number; pushing it out by the added days
    // keeps due and interval consistent. `saturating_add` guards the (unreached
    // in practice) i32 overflow.
    updated.due = updated.due.saturating_add(delta as i32);

    // If the nudge produced no change (e.g. interval was 0/1 and rounding
    // yielded the same value and stability was ~0), skip the write.
    if delta == 0 && updated.memory_state == card.memory_state {
        return None;
    }
    Some(updated)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::card::CardType;
    use crate::card::FsrsMemoryState;

    fn add_card(col: &mut Collection, tag: &str) -> CardId {
        let nt = col.get_notetype_by_name("Basic").unwrap().unwrap();
        let mut note = nt.new_note();
        note.set_field(0, "q").unwrap();
        note.tags = vec![tag.to_string()];
        col.add_note(&mut note, DeckId(1)).unwrap();
        col.storage
            .all_card_ids_of_note_in_template_order(note.id)
            .unwrap()[0]
    }

    fn make_review(col: &mut Collection, cid: CardId, interval: u32, stability: f32) {
        let orig = col.storage.get_card(cid).unwrap().unwrap();
        let mut card = orig.clone();
        card.ctype = CardType::Review;
        card.queue = CardQueue::Review;
        card.interval = interval;
        card.due = 100;
        card.memory_state = Some(FsrsMemoryState {
            stability,
            difficulty: 5.0,
        });
        col.update_card_inner(&mut card, orig, col.usn().unwrap())
            .unwrap();
    }

    #[test]
    fn credit_strengthens_prereq_review_card() -> Result<()> {
        let mut col = Collection::new();
        // amino_acids is a prerequisite of protein_structure (built in-test, as
        // the production spine seed is now empty).
        let prereq_card = add_card(&mut col, "concept::BB::1A::amino_acids");
        let app_card = add_card(&mut col, "concept::BB::1A::protein_structure");
        super::super::edges::add_test_concept_edge(
            &col,
            "concept::BB::1A::amino_acids",
            "concept::BB::1A::protein_structure",
        );
        make_review(&mut col, prereq_card, 20, 40.0);

        let before = col.storage.get_card(prereq_card)?.unwrap();
        col.apply_trickle_down_credit(app_card)?;
        let after = col.storage.get_card(prereq_card)?.unwrap();

        assert!(
            after.memory_state.unwrap().stability > before.memory_state.unwrap().stability,
            "stability should increase"
        );
        assert!(after.interval > before.interval, "interval should increase");
        assert!(after.due > before.due, "due should be pushed out");
        Ok(())
    }

    #[test]
    fn credit_skips_new_and_learning_cards() -> Result<()> {
        let mut col = Collection::new();
        let prereq_card = add_card(&mut col, "concept::BB::1A::amino_acids");
        let app_card = add_card(&mut col, "concept::BB::1A::protein_structure");
        super::super::edges::add_test_concept_edge(
            &col,
            "concept::BB::1A::amino_acids",
            "concept::BB::1A::protein_structure",
        );
        // prereq card left New (no memory state).
        let before = col.storage.get_card(prereq_card)?.unwrap();
        col.apply_trickle_down_credit(app_card)?;
        let after = col.storage.get_card(prereq_card)?.unwrap();
        assert_eq!(before.interval, after.interval);
        assert_eq!(before.due, after.due);
        assert!(after.memory_state.is_none());
        Ok(())
    }

    #[test]
    fn credit_noop_when_no_prerequisites() -> Result<()> {
        let mut col = Collection::new();
        // amino_acids has no prerequisites of its own.
        let leaf_prereq = add_card(&mut col, "concept::BB::1A::amino_acids");
        make_review(&mut col, leaf_prereq, 20, 40.0);
        let before = col.storage.get_card(leaf_prereq)?.unwrap();
        // answering amino_acids grants nothing (it depends on nothing).
        col.apply_trickle_down_credit(leaf_prereq)?;
        let after = col.storage.get_card(leaf_prereq)?.unwrap();
        assert_eq!(before.interval, after.interval);
        assert_eq!(before.memory_state, after.memory_state);
        Ok(())
    }
}
