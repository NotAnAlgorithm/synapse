// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Synapse (M2, workstream B): card metamorphosis, "add-then-fade" (PRD B3).
//!
//! When a student masters the *application* form of a concept, the bare-recall
//! cards for that same concept are FADED: suspended (reversible) and stamped
//! with a `custom_data` marker so the fade is idempotent and can be undone. We
//! NEVER delete recall cards, and we NEVER auto-generate application items.
//! This is deliberately conservative — expertise-reversal does not license
//! retiring retrieval practice, and Bjork's New Theory of Disuse warns that a
//! retired fact quietly rots before a fixed-date exam (see PRD B3). Suspension
//! keeps the substrate intact and cheap while removing it from daily review;
//! the pre-exam governor (workstream A) can re-surface it.
//!
//! These are STANDALONE `Collection` methods. The integrator calls
//! [`Collection::apply_metamorphosis_after_answer`] from the answer path behind
//! workstream A's deck-config flag — this module does not touch
//! `scheduler/answering/mod.rs` or the queue builder, and defines no
//! deck-config flag of its own.
//!
//! Concept membership: this base branch's concept layer is the M0
//! `concept::<section>::<id>` note-tag convention, so membership is resolved
//! via note tags here. INTEGRATOR: once M1's `card_concepts` table is present,
//! swap [`Collection::recall_cards_sharing_concepts`]'s tag walk for a
//! `card_concepts` join; the public method surface is unchanged.

use std::collections::HashMap;
use std::collections::HashSet;

use serde_json::Map;
use serde_json::Value;

use crate::card::CardQueue;
use crate::prelude::*;
use crate::search::JoinSearches;
use crate::search::SearchNode;

/// Note-tag prefix identifying a concept tag (matches M0's Memory read-model in
/// `stats/concepts.rs`). Kept in sync manually; both are the M0 concept layer.
const CONCEPT_TAG_PREFIX: &str = "concept::";

/// Notetype-name prefix marking an *application* item. Shared classification
/// with M1's interleaving heuristic (a notetype named "MCAT ..." is an
/// application item; anything else is recall). INTEGRATOR: factor this and
/// interleave.rs's copy into one shared helper; behavior must stay identical.
const APPLICATION_NOTETYPE_PREFIX: &str = "MCAT ";

/// `custom_data` key marking a recall card faded by metamorphosis. 4 bytes
/// (<=8), value `1`; fits alongside mint.py's `"src"` within the 100-byte cap.
/// Presence of the key is the single source of truth for "faded by us", used
/// for both idempotency and scoped reversal.
const FADE_KEY: &str = "fade";

// --- Tunable mastery thresholds (conservative by design) --------------------
//
// Mastery is judged on the APPLICATION card and requires BOTH a stable memory
// and a good track record, over enough reps to be trustworthy. These are
// intentionally cautious; tune against real outcome data (see PRD B3 success
// criterion: mastered concepts must stay retrievable through test day).

/// Minimum FSRS stability (days) on the application card to count as mastered.
/// ~60d means the model expects the fact to survive roughly a two-month gap.
const MASTERY_MIN_STABILITY_DAYS: f32 = 60.0;
/// Minimum accuracy (correct / total reps) on the application card.
const MASTERY_MIN_ACCURACY: f32 = 0.85;
/// Minimum number of reps before accuracy is trusted at all.
const MASTERY_MIN_REPS: u32 = 3;

/// How a recall card is classified relative to a concept's application form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionType {
    /// Notetype name starts with "MCAT " (an application/passage item).
    Application,
    /// Everything else (bare recall).
    Recall,
}

/// Classify a notetype name into application vs recall. Shared heuristic; see
/// [`APPLICATION_NOTETYPE_PREFIX`].
pub fn question_type_for_notetype_name(name: &str) -> QuestionType {
    if name.starts_with(APPLICATION_NOTETYPE_PREFIX) {
        QuestionType::Application
    } else {
        QuestionType::Recall
    }
}

/// Outcome of a metamorphosis pass, for logging/telemetry by the caller.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetamorphosisOutcome {
    /// Recall cards newly faded (suspended) by this call.
    pub faded: usize,
    /// Recall cards already faded and left untouched (idempotent no-ops).
    pub already_faded: usize,
}

impl Card {
    /// True if this card was faded by metamorphosis (carries the fade marker).
    fn is_faded(&self) -> bool {
        custom_data_has_fade(&self.custom_data)
    }
}

impl Collection {
    /// Entry point for the answer path. Given the card that was just answered,
    /// if it is a mastered APPLICATION card, fade the recall cards of every
    /// concept it belongs to. No-op if the card isn't an application card,
    /// isn't mastered, or has no concept tags. Idempotent.
    ///
    /// INTEGRATOR: call this from `answer_card_inner` behind workstream A's
    /// metamorphosis deck-config flag. Returns the outcome for telemetry.
    pub fn apply_metamorphosis_after_answer(
        &mut self,
        card_id: CardId,
    ) -> Result<OpOutput<MetamorphosisOutcome>> {
        self.transact(Op::UpdateCard, |col| {
            col.apply_metamorphosis_after_answer_inner(card_id)
        })
    }

    pub(crate) fn apply_metamorphosis_after_answer_inner(
        &mut self,
        card_id: CardId,
    ) -> Result<MetamorphosisOutcome> {
        let card = self.storage.get_card(card_id)?.or_not_found(card_id)?;

        // Must be an application card to trigger a fade.
        if self.question_type_of_card(&card)? != QuestionType::Application {
            return Ok(MetamorphosisOutcome::default());
        }
        // Must be mastered on this application form.
        if !application_card_is_mastered(&card) {
            return Ok(MetamorphosisOutcome::default());
        }

        let concepts = self.concept_tags_of_card(&card)?;
        if concepts.is_empty() {
            return Ok(MetamorphosisOutcome::default());
        }

        self.fade_recall_cards_for_concepts(&concepts, Some(card.note_id))
    }

    /// Fade (suspend + mark) the recall cards belonging to any of `concepts`,
    /// excluding cards on `exclude_note` (typically the application note that
    /// triggered the fade — though it wouldn't match the recall filter anyway).
    /// Idempotent: already-faded cards are counted but left untouched.
    fn fade_recall_cards_for_concepts(
        &mut self,
        concepts: &HashSet<String>,
        exclude_note: Option<NoteId>,
    ) -> Result<MetamorphosisOutcome> {
        let usn = self.usn()?;
        let mut outcome = MetamorphosisOutcome::default();

        for card in self.recall_cards_sharing_concepts(concepts)? {
            if Some(card.note_id) == exclude_note {
                continue;
            }
            if card.is_faded() {
                outcome.already_faded += 1;
                continue;
            }
            let original = card.clone();
            let mut card = card;
            card.queue = CardQueue::Suspended;
            card.custom_data = with_fade_marker(&card.custom_data)?;
            self.update_card_inner(&mut card, original, usn)?;
            outcome.faded += 1;
        }

        Ok(outcome)
    }

    /// Reverse a fade for every recall card of `concepts`: unsuspend and clear
    /// the marker. Idempotent no-op for cards we did not fade. Reversible
    /// counterpart to metamorphosis (never touches user-suspended cards, since
    /// those lack the marker).
    pub fn reverse_metamorphosis_for_concepts(
        &mut self,
        concepts: &HashSet<String>,
    ) -> Result<OpOutput<usize>> {
        self.transact(Op::UpdateCard, |col| {
            let usn = col.usn()?;
            let mut count = 0;
            for card in col.recall_cards_sharing_concepts(concepts)? {
                if !card.is_faded() {
                    continue;
                }
                let original = card.clone();
                let mut card = card;
                card.custom_data = without_fade_marker(&card.custom_data)?;
                // Restore the natural queue only if we had suspended it.
                if card.queue == CardQueue::Suspended {
                    card.restore_queue_from_type();
                }
                col.update_card_inner(&mut card, original, usn)?;
                count += 1;
            }
            Ok(count)
        })
    }

    /// All RECALL cards whose note carries at least one of `concepts`. Concept
    /// membership uses the M0 `concept::` note-tag layer (see module docs): we
    /// search for cards tagged with any of the concept tags, then drop the
    /// application-type ones. INTEGRATOR: swap the tag search for a
    /// `card_concepts` join once M1's table is present.
    fn recall_cards_sharing_concepts(&mut self, concepts: &HashSet<String>) -> Result<Vec<Card>> {
        if concepts.is_empty() {
            return Ok(vec![]);
        }
        // Deterministic search: OR every concept tag together.
        let mut tags: Vec<&String> = concepts.iter().collect();
        tags.sort();
        let mut search = SearchBuilder::new();
        for tag in tags {
            search = search.or(SearchNode::from_tag_name(tag));
        }

        let mut note_qtype: HashMap<NoteId, QuestionType> = HashMap::new();
        let mut out = Vec::new();
        for card in self.all_cards_for_search(search)? {
            let qtype = match note_qtype.get(&card.note_id) {
                Some(q) => *q,
                None => {
                    let q = self.question_type_of_note(card.note_id)?;
                    note_qtype.insert(card.note_id, q);
                    q
                }
            };
            if qtype == QuestionType::Recall {
                out.push(card);
            }
        }
        out.sort_by_key(|c| c.id);
        Ok(out)
    }

    /// Concept tags (full `concept::...` strings) on a card's note.
    fn concept_tags_of_card(&mut self, card: &Card) -> Result<HashSet<String>> {
        let tags = self
            .storage
            .get_note_without_fields(card.note_id)?
            .map(|n| n.tags)
            .unwrap_or_default();
        Ok(tags
            .into_iter()
            .filter(|t| t.starts_with(CONCEPT_TAG_PREFIX))
            .collect())
    }

    fn question_type_of_card(&mut self, card: &Card) -> Result<QuestionType> {
        self.question_type_of_note(card.note_id)
    }

    fn question_type_of_note(&mut self, nid: NoteId) -> Result<QuestionType> {
        let note = match self.storage.get_note_without_fields(nid)? {
            Some(n) => n,
            None => return Ok(QuestionType::Recall),
        };
        let name = self
            .get_notetype(note.notetype_id)?
            .map(|nt| nt.name.clone())
            .unwrap_or_default();
        Ok(question_type_for_notetype_name(&name))
    }
}

/// Conservative mastery check on an application card: stable memory AND a good,
/// sufficiently-long track record. See the `MASTERY_*` consts.
fn application_card_is_mastered(card: &Card) -> bool {
    let Some(state) = card.memory_state else {
        return false;
    };
    if state.stability < MASTERY_MIN_STABILITY_DAYS {
        return false;
    }
    if card.reps < MASTERY_MIN_REPS {
        return false;
    }
    let correct = card.reps.saturating_sub(card.lapses);
    let accuracy = correct as f32 / card.reps as f32;
    accuracy >= MASTERY_MIN_ACCURACY
}

// --- custom_data fade-marker helpers ----------------------------------------

fn parse_custom_data(custom_data: &str) -> Map<String, Value> {
    if custom_data.is_empty() {
        return Map::new();
    }
    match serde_json::from_str::<Value>(custom_data) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

fn serialize_custom_data(map: &Map<String, Value>) -> Result<String> {
    if map.is_empty() {
        return Ok(String::new());
    }
    serde_json::to_string(map).map_err(Into::into)
}

fn custom_data_has_fade(custom_data: &str) -> bool {
    parse_custom_data(custom_data).contains_key(FADE_KEY)
}

/// Return `custom_data` with the fade marker added (idempotent).
fn with_fade_marker(custom_data: &str) -> Result<String> {
    let mut map = parse_custom_data(custom_data);
    map.insert(FADE_KEY.to_string(), Value::from(1));
    serialize_custom_data(&map)
}

/// Return `custom_data` with the fade marker removed (idempotent).
fn without_fade_marker(custom_data: &str) -> Result<String> {
    let mut map = parse_custom_data(custom_data);
    map.remove(FADE_KEY);
    serialize_custom_data(&map)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::card::CardType;
    use crate::card::FsrsMemoryState;

    const CONCEPT: &str = "concept::biochem::amino_acid_charge";

    /// Add a notetype named `name` by cloning Basic; returns its id.
    fn add_named_notetype(col: &mut Collection, name: &str) -> NotetypeId {
        let mut nt = col.basic_notetype();
        nt.id = NotetypeId(0);
        nt.name = name.to_string();
        col.add_notetype(&mut nt, true).unwrap();
        nt.id
    }

    /// Add a note of the given notetype tagged with CONCEPT, return its single
    /// card id.
    fn add_tagged_card(col: &mut Collection, ntid: NotetypeId) -> CardId {
        let nt = col.get_notetype(ntid).unwrap().unwrap();
        let mut note = nt.new_note();
        note.tags = vec![CONCEPT.to_string()];
        col.add_note(&mut note, DeckId(1)).unwrap();
        col.storage.all_cards_of_note(note.id).unwrap()[0].id
    }

    /// Force a card into a "mastered application" state.
    fn make_mastered(col: &mut Collection, card_id: CardId) {
        col.get_and_update_card(card_id, |c| {
            c.ctype = CardType::Review;
            c.queue = CardQueue::Review;
            c.reps = 6;
            c.lapses = 0;
            c.memory_state = Some(FsrsMemoryState {
                stability: 120.0,
                difficulty: 5.0,
            });
            Ok(())
        })
        .unwrap();
    }

    fn queue_of(col: &Collection, card_id: CardId) -> CardQueue {
        col.storage.get_card(card_id).unwrap().unwrap().queue
    }

    fn is_faded(col: &Collection, card_id: CardId) -> bool {
        col.storage.get_card(card_id).unwrap().unwrap().is_faded()
    }

    #[test]
    fn fades_recall_once_application_mastered() {
        let mut col = Collection::new();
        let mcat = add_named_notetype(&mut col, "MCAT Application");
        let app = add_tagged_card(&mut col, mcat);
        // Recall card on the default Basic notetype, same concept.
        let basic = col.basic_notetype().id;
        let recall = add_tagged_card(&mut col, basic);

        // Not mastered yet -> no-op.
        let out = col.apply_metamorphosis_after_answer(app).unwrap().output;
        assert_eq!(out, MetamorphosisOutcome::default());
        assert_eq!(queue_of(&col, recall), CardQueue::New);
        assert!(!is_faded(&col, recall));

        // Master the application card -> recall fades.
        make_mastered(&mut col, app);
        let out = col.apply_metamorphosis_after_answer(app).unwrap().output;
        assert_eq!(out.faded, 1);
        assert_eq!(out.already_faded, 0);
        assert_eq!(queue_of(&col, recall), CardQueue::Suspended);
        assert!(is_faded(&col, recall));
        // Application card itself is untouched (never faded).
        assert!(!is_faded(&col, app));

        // Idempotent: second call fades nothing new.
        let out = col.apply_metamorphosis_after_answer(app).unwrap().output;
        assert_eq!(out.faded, 0);
        assert_eq!(out.already_faded, 1);
        assert_eq!(queue_of(&col, recall), CardQueue::Suspended);

        // Reversible: restores the recall card and clears the marker.
        let mut set = HashSet::new();
        set.insert(CONCEPT.to_string());
        let restored = col.reverse_metamorphosis_for_concepts(&set).unwrap().output;
        assert_eq!(restored, 1);
        assert!(!is_faded(&col, recall));
        assert_ne!(queue_of(&col, recall), CardQueue::Suspended);

        // Reverse is idempotent (nothing left to restore).
        let restored = col.reverse_metamorphosis_for_concepts(&set).unwrap().output;
        assert_eq!(restored, 0);
    }

    #[test]
    fn no_fade_when_no_shared_concept() {
        let mut col = Collection::new();
        let mcat = add_named_notetype(&mut col, "MCAT Application");
        let app = add_tagged_card(&mut col, mcat);
        make_mastered(&mut col, app);

        // A recall card with a DIFFERENT concept tag must not be faded.
        let basic = col.basic_notetype().id;
        let nt = col.get_notetype(basic).unwrap().unwrap();
        let mut other = nt.new_note();
        other.tags = vec!["concept::physio::other".to_string()];
        col.add_note(&mut other, DeckId(1)).unwrap();
        let other_card = col.storage.all_cards_of_note(other.id).unwrap()[0].id;

        let out = col.apply_metamorphosis_after_answer(app).unwrap().output;
        assert_eq!(out.faded, 0);
        assert!(!is_faded(&col, other_card));
    }

    #[test]
    fn non_application_card_never_triggers_fade() {
        let mut col = Collection::new();
        let basic = col.basic_notetype().id;
        let recall_a = add_tagged_card(&mut col, basic);
        let recall_b = add_tagged_card(&mut col, basic);
        // Even if a recall card is "mastered", answering it triggers nothing:
        // only application cards drive metamorphosis.
        make_mastered(&mut col, recall_a);
        let out = col
            .apply_metamorphosis_after_answer(recall_a)
            .unwrap()
            .output;
        assert_eq!(out, MetamorphosisOutcome::default());
        assert!(!is_faded(&col, recall_b));
    }

    #[test]
    fn classify() {
        assert_eq!(
            question_type_for_notetype_name("MCAT Application"),
            QuestionType::Application
        );
        assert_eq!(
            question_type_for_notetype_name("MCAT Which-Principle"),
            QuestionType::Application
        );
        assert_eq!(
            question_type_for_notetype_name("Basic"),
            QuestionType::Recall
        );
        // A leading-space-less "MCAT" (no trailing space) is NOT application.
        assert_eq!(
            question_type_for_notetype_name("MCATish"),
            QuestionType::Recall
        );
    }

    #[test]
    fn fade_marker_roundtrip() {
        // empty -> add -> present
        let added = with_fade_marker("").unwrap();
        assert!(custom_data_has_fade(&added));
        // idempotent add
        assert_eq!(with_fade_marker(&added).unwrap(), added);
        // preserves existing keys (e.g. mint.py's "src")
        let with_src = with_fade_marker(r#"{"src":123}"#).unwrap();
        assert!(custom_data_has_fade(&with_src));
        let map = parse_custom_data(&with_src);
        assert_eq!(map.get("src"), Some(&Value::from(123)));
        // remove -> gone, src preserved, and empty collapses to ""
        let removed = without_fade_marker(&with_src).unwrap();
        assert!(!custom_data_has_fade(&removed));
        assert_eq!(
            parse_custom_data(&removed).get("src"),
            Some(&Value::from(123))
        );
        assert_eq!(without_fade_marker(&added).unwrap(), "");
        // idempotent remove
        assert_eq!(without_fade_marker("").unwrap(), "");
    }

    #[test]
    fn mastery_threshold() {
        use crate::card::FsrsMemoryState;
        let mut card = Card {
            reps: 5,
            lapses: 0,
            memory_state: Some(FsrsMemoryState {
                stability: 90.0,
                difficulty: 5.0,
            }),
            ..Default::default()
        };
        assert!(application_card_is_mastered(&card));
        // low stability fails
        card.memory_state = Some(FsrsMemoryState {
            stability: 10.0,
            difficulty: 5.0,
        });
        assert!(!application_card_is_mastered(&card));
        // stable but too many lapses fails
        card.memory_state = Some(FsrsMemoryState {
            stability: 90.0,
            difficulty: 5.0,
        });
        card.lapses = 4; // 1/5 = 0.2 accuracy
        assert!(!application_card_is_mastered(&card));
        // stable, accurate, but too few reps fails
        card.lapses = 0;
        card.reps = 2;
        assert!(!application_card_is_mastered(&card));
        // no memory state fails
        card.reps = 5;
        card.memory_state = None;
        assert!(!application_card_is_mastered(&card));
    }
}
