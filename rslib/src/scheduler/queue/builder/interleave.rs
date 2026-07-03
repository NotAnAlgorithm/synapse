// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Synapse concept + question-type interleaving.
//!
//! When the deck-config toggle `interleave_by_concept` is enabled, the v3
//! queue builder runs a spreading (round-robin) pass over the already-assembled
//! main queue so that consecutive cards alternate by concept/section *and* by
//! question-type (recall vs application) as much as the material allows.
//!
//! This is a *pure reordering* of the cards the builder already selected: no
//! card is ever dropped or duplicated, limits/selection are untouched, and
//! when the toggle is off none of this code runs (behaviour is byte-for-byte
//! unchanged).
//!
//! Classification inputs (both derived from data the collection already syncs):
//! - **section**: the second `::` segment of the note's first `concept::` tag,
//!   matching the `concept::<section>::<id>` convention seeded in M0 (see
//!   `stats/concepts.rs`). Cards with no concept tag get `section = None`.
//! - **question-type**: derived from the notetype name. "MCAT Application" and
//!   any notetype whose name starts with `MCAT ` are treated as *application*;
//!   everything else (Basic, Cloze, ...) is treated as *recall*. This mirrors
//!   the provisioning convention where application-style items live under the
//!   `MCAT ` notetype namespace.

use std::collections::HashMap;

use super::QueueBuilder;
use crate::prelude::*;

/// Note-tag prefix identifying a concept tag, e.g.
/// `concept::biochem::amino_acid_charge`. Kept in sync with the same constant
/// in `stats/concepts.rs` (both derive from the M0 tag convention).
const CONCEPT_TAG_PREFIX: &str = "concept::";
/// Notetype-name prefix marking an application-style item.
const APPLICATION_NOTETYPE_PREFIX: &str = "MCAT ";

/// Question-type bucket used for interleaving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum QuestionType {
    /// Straight recall (Basic, Cloze, ...).
    Recall,
    /// Application-style item (MCAT Application and other `MCAT ` notetypes).
    Application,
}

/// The interleaving key for a single card: which concept section it belongs to
/// (interned to a small integer; `None` when the note carries no concept tag)
/// and whether it is a recall or application item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct InterleaveKey {
    /// Interned section index; `None` when the note has no `concept::` tag.
    pub(super) section: Option<u32>,
    pub(super) qtype: QuestionType,
}

/// Classify a note's tags/notetype name into an [`InterleaveKey`].
///
/// `sections` interns section names to stable small integers so keys are cheap
/// to compare and hash.
fn classify(
    tags: &[String],
    notetype_name: &str,
    sections: &mut HashMap<String, u32>,
) -> InterleaveKey {
    let section = section_from_tags(tags).map(|name| {
        let next = sections.len() as u32;
        *sections.entry(name.to_string()).or_insert(next)
    });
    let qtype = if is_application_notetype(notetype_name) {
        QuestionType::Application
    } else {
        QuestionType::Recall
    };
    InterleaveKey { section, qtype }
}

/// Extract the section (2nd `::` segment) of the first `concept::` tag, if any.
fn section_from_tags(tags: &[String]) -> Option<&str> {
    tags.iter()
        .filter_map(|tag| tag.strip_prefix(CONCEPT_TAG_PREFIX))
        .find_map(|rest| {
            // `rest` is `<section>::<id>`; the section is the first segment and
            // must be non-empty to be meaningful.
            let section = rest.split("::").next().unwrap_or("");
            (!section.is_empty()).then_some(section)
        })
}

/// Heuristic: application-style notetypes are "MCAT Application" and any type
/// whose name begins with `MCAT `. Everything else is treated as recall.
fn is_application_notetype(name: &str) -> bool {
    name.starts_with(APPLICATION_NOTETYPE_PREFIX)
}

impl QueueBuilder {
    /// Build the `card_id -> InterleaveKey` map for every card the builder has
    /// gathered. Called only when the toggle is on. Note tags and notetype
    /// names are cached so repeated notes/notetypes cost a single lookup each.
    pub(super) fn classify_for_interleave(&mut self, col: &mut Collection) -> Result<()> {
        let mut note_key_cache: HashMap<NoteId, InterleaveKey> = HashMap::new();
        let mut notetype_name_cache: HashMap<NotetypeId, String> = HashMap::new();
        let mut sections: HashMap<String, u32> = HashMap::new();

        // (card_id, note_id) for every gathered card across all main-queue
        // pools. Intraday learning is not part of `main`, so it is excluded.
        let entries: Vec<(CardId, NoteId)> = self
            .new
            .iter()
            .map(|c| (c.id, c.note_id))
            .chain(self.review.iter().map(|c| (c.id, c.note_id)))
            .chain(self.day_learning.iter().map(|c| (c.id, c.note_id)))
            .collect();

        for (card_id, note_id) in entries {
            let key = match note_key_cache.get(&note_id) {
                Some(key) => *key,
                None => {
                    let key =
                        self.classify_note(col, note_id, &mut notetype_name_cache, &mut sections)?;
                    note_key_cache.insert(note_id, key);
                    key
                }
            };
            self.interleave_keys.insert(card_id, key);
        }

        Ok(())
    }

    fn classify_note(
        &self,
        col: &mut Collection,
        note_id: NoteId,
        notetype_name_cache: &mut HashMap<NotetypeId, String>,
        sections: &mut HashMap<String, u32>,
    ) -> Result<InterleaveKey> {
        // A missing note shouldn't happen for a gathered card, but fall back to
        // a plain recall/no-section key rather than failing the whole build.
        let Some(note) = col.storage.get_note_without_fields(note_id)? else {
            return Ok(InterleaveKey {
                section: None,
                qtype: QuestionType::Recall,
            });
        };
        let notetype_id = note.notetype_id;
        let notetype_name = match notetype_name_cache.get(&notetype_id) {
            Some(name) => name.as_str(),
            None => {
                let name = col
                    .storage
                    .get_notetype(notetype_id)?
                    .map(|nt| nt.name)
                    .unwrap_or_default();
                notetype_name_cache.entry(notetype_id).or_insert(name)
            }
        };
        Ok(classify(&note.tags, notetype_name, sections))
    }
}

/// Reorder `entries` in place with a spreading (round-robin) pass so that
/// consecutive entries differ in their interleave key wherever the material
/// allows.
///
/// Contract:
/// - the multiset of entries is preserved exactly (no drops, no duplicates);
/// - entries sharing a key keep their original relative order (stable per
///   bucket);
/// - when every remaining entry shares the last emitted key (e.g. a single
///   concept/type is left), they are emitted in their original order.
///
/// Algorithm: bucket the entries by key (preserving arrival order within each
/// bucket), then greedily emit. At each step pick, among buckets whose key
/// differs from the previously-emitted key, the bucket with the most remaining
/// entries (spreading the largest group out); ties are broken to prefer a
/// different question-type, then a different section, then the bucket that
/// first appeared (for determinism). If no differing bucket has entries, emit
/// from the (only) remaining bucket.
///
/// `key_of` maps an entry to its key; entries with no known key are treated as
/// a distinct "unclassified" bucket so they too get spread out rather than
/// clumping.
pub(super) fn interleave_entries<T: Copy>(
    entries: &mut Vec<T>,
    key_of: impl Fn(&T) -> Option<InterleaveKey>,
) {
    if entries.len() < 3 {
        // 0/1 entries: nothing to do. 2 entries: already maximally alternating
        // (or unavoidably identical) — leave order untouched to stay minimal.
        return;
    }

    // Bucket entries by key, preserving arrival order within each bucket and
    // recording first-appearance order across buckets for a stable tie-break.
    let mut buckets: Vec<Bucket<T>> = Vec::new();
    let mut key_to_bucket: HashMap<Option<InterleaveKey>, usize> = HashMap::new();
    for (arrival, entry) in entries.iter().enumerate() {
        let key = key_of(entry);
        let idx = *key_to_bucket.entry(key).or_insert_with(|| {
            buckets.push(Bucket {
                key,
                items: Vec::new(),
                first_seen: arrival,
                next: 0,
            });
            buckets.len() - 1
        });
        buckets[idx].items.push(*entry);
    }

    // Nothing to interleave if everything shares one key.
    if buckets.len() < 2 {
        return;
    }

    let total = entries.len();
    let mut out: Vec<T> = Vec::with_capacity(total);
    let mut last_key: Option<Option<InterleaveKey>> = None;

    for _ in 0..total {
        let choice = pick_bucket(&buckets, last_key);
        let bucket = &mut buckets[choice];
        out.push(bucket.items[bucket.next]);
        bucket.next += 1;
        last_key = Some(bucket.key);
    }

    debug_assert_eq!(out.len(), entries.len());
    *entries = out;
}

struct Bucket<T> {
    key: Option<InterleaveKey>,
    items: Vec<T>,
    /// Index of the first entry in the original queue, for a stable tie-break.
    first_seen: usize,
    /// How many items have been emitted from this bucket.
    next: usize,
}

impl<T> Bucket<T> {
    fn remaining(&self) -> usize {
        self.items.len() - self.next
    }
}

/// Choose the next bucket to emit from, given the previously-emitted key.
fn pick_bucket<T>(buckets: &[Bucket<T>], last_key: Option<Option<InterleaveKey>>) -> usize {
    // Prefer buckets whose key differs from the last emitted one.
    let differs = |b: &Bucket<T>| last_key.map(|lk| lk != b.key).unwrap_or(true);

    let mut best: Option<usize> = None;
    for (idx, bucket) in buckets.iter().enumerate() {
        if bucket.remaining() == 0 {
            continue;
        }
        best = Some(match best {
            None => idx,
            Some(cur) => {
                if better_choice(
                    bucket,
                    &buckets[cur],
                    last_key,
                    differs(bucket),
                    differs(&buckets[cur]),
                ) {
                    idx
                } else {
                    cur
                }
            }
        });
    }
    best.expect("at least one bucket has remaining items")
}

/// Returns true if `cand` is a better next pick than `cur`.
fn better_choice<T>(
    cand: &Bucket<T>,
    cur: &Bucket<T>,
    last_key: Option<Option<InterleaveKey>>,
    cand_differs: bool,
    cur_differs: bool,
) -> bool {
    // 1. A bucket that differs from the last key always beats one that doesn't.
    if cand_differs != cur_differs {
        return cand_differs;
    }
    // 2. Spread the largest group out first.
    if cand.remaining() != cur.remaining() {
        return cand.remaining() > cur.remaining();
    }
    // 3. Among equal-size differing buckets, prefer flipping question-type, then
    //    flipping section, relative to the last emitted key.
    if let Some(Some(last)) = last_key {
        let cand_score = flip_score(cand.key, last);
        let cur_score = flip_score(cur.key, last);
        if cand_score != cur_score {
            return cand_score > cur_score;
        }
    }
    // 4. Deterministic final tie-break: earliest original appearance.
    cand.first_seen < cur.first_seen
}

/// Higher = more desirable flip relative to `last`: reward changing the
/// question-type most, then changing the section.
fn flip_score(key: Option<InterleaveKey>, last: InterleaveKey) -> u8 {
    let Some(key) = key else {
        // Unclassified bucket: neutral score.
        return 0;
    };
    let mut score = 0;
    if key.qtype != last.qtype {
        score += 2;
    }
    if key.section != last.section {
        score += 1;
    }
    score
}

#[cfg(test)]
mod test {
    use super::*;

    fn rec(section: Option<u32>) -> InterleaveKey {
        InterleaveKey {
            section,
            qtype: QuestionType::Recall,
        }
    }

    fn app(section: Option<u32>) -> InterleaveKey {
        InterleaveKey {
            section,
            qtype: QuestionType::Application,
        }
    }

    /// Helper: reorder (value, key) pairs and return the values.
    fn run(pairs: Vec<(u32, InterleaveKey)>) -> Vec<u32> {
        let mut items = pairs;
        interleave_entries(&mut items, |(_, k)| Some(*k));
        items.into_iter().map(|(v, _)| v).collect()
    }

    #[test]
    fn classify_extracts_section_and_qtype() {
        let mut sections = HashMap::new();
        let k = classify(
            &["concept::biochem::amino_acid_charge".to_string()],
            "MCAT Application",
            &mut sections,
        );
        assert_eq!(k.qtype, QuestionType::Application);
        assert!(k.section.is_some());

        let k2 = classify(
            &["concept::physics::optics".to_string()],
            "Basic",
            &mut sections,
        );
        assert_eq!(k2.qtype, QuestionType::Recall);
        // distinct section interned to a distinct id
        assert_ne!(k.section, k2.section);

        // same section reuses the interned id
        let k3 = classify(
            &["concept::biochem::something_else".to_string()],
            "Cloze",
            &mut sections,
        );
        assert_eq!(k3.section, k.section);

        // no concept tag -> no section
        let k4 = classify(&["leech".to_string()], "Basic", &mut sections);
        assert_eq!(k4.section, None);
        assert_eq!(k4.qtype, QuestionType::Recall);
    }

    #[test]
    fn application_notetype_heuristic() {
        assert!(is_application_notetype("MCAT Application"));
        assert!(is_application_notetype("MCAT Data Snippet"));
        assert!(!is_application_notetype("Basic"));
        assert!(!is_application_notetype("Cloze"));
        assert!(!is_application_notetype("MCAT")); // no trailing space
    }

    #[test]
    fn preserves_multiset_and_within_bucket_order() {
        // Two sections, both recall; interleaving must keep every value and the
        // relative order within each section.
        let out = run(vec![
            (1, rec(Some(0))),
            (2, rec(Some(0))),
            (3, rec(Some(0))),
            (4, rec(Some(1))),
            (5, rec(Some(1))),
            (6, rec(Some(1))),
        ]);
        let mut sorted = out.clone();
        sorted.sort();
        assert_eq!(sorted, vec![1, 2, 3, 4, 5, 6]);
        // relative order within section 0 preserved: 1 before 2 before 3
        let pos = |v| out.iter().position(|&x| x == v).unwrap();
        assert!(pos(1) < pos(2) && pos(2) < pos(3));
        assert!(pos(4) < pos(5) && pos(5) < pos(6));
    }

    #[test]
    fn alternates_two_equal_sections() {
        let out = run(vec![
            (1, rec(Some(0))),
            (2, rec(Some(0))),
            (3, rec(Some(0))),
            (4, rec(Some(1))),
            (5, rec(Some(1))),
            (6, rec(Some(1))),
        ]);
        // No two adjacent cards should share a section when both have material.
        // With 3+3 this is perfectly achievable.
        assert_eq!(out, vec![1, 4, 2, 5, 3, 6]);
    }

    #[test]
    fn alternates_question_type() {
        // One section, but recall + application: should alternate by type.
        let out = run(vec![
            (1, rec(Some(0))),
            (2, rec(Some(0))),
            (3, rec(Some(0))),
            (4, app(Some(0))),
            (5, app(Some(0))),
            (6, app(Some(0))),
        ]);
        assert_eq!(out, vec![1, 4, 2, 5, 3, 6]);
    }

    #[test]
    fn single_bucket_unchanged() {
        let out = run(vec![
            (1, rec(Some(0))),
            (2, rec(Some(0))),
            (3, rec(Some(0))),
            (4, rec(Some(0))),
        ]);
        assert_eq!(out, vec![1, 2, 3, 4]);
    }

    #[test]
    fn short_queues_untouched() {
        assert_eq!(run(vec![(1, rec(Some(0))), (2, rec(Some(1)))]), vec![1, 2]);
        assert_eq!(run(vec![(1, rec(Some(0)))]), vec![1]);
        assert_eq!(run(Vec::new()), Vec::<u32>::new());
    }

    #[test]
    fn uneven_groups_spread_largest() {
        // Section 0 dominates; it should be spread across the queue rather than
        // clumped, and no card is lost.
        let out = run(vec![
            (1, rec(Some(0))),
            (2, rec(Some(0))),
            (3, rec(Some(0))),
            (4, rec(Some(0))),
            (5, rec(Some(1))),
        ]);
        let mut sorted = out.clone();
        sorted.sort();
        assert_eq!(sorted, vec![1, 2, 3, 4, 5]);
        // The lone section-1 card (5) should not be at either extreme end,
        // i.e. the dominant group is spread around it.
        let pos5 = out.iter().position(|&x| x == 5).unwrap();
        assert!(pos5 > 0 && pos5 < out.len() - 1);
    }
}
