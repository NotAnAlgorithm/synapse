// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Storage layer for the Synapse concept projection.
//!
//! The canonical `concept::<section>::<category>::<topic>` note-tag convention
//! (e.g. `concept::BB::1A::amino_acids`) is the source of truth for the concept
//! layer, keyed to the AAMC MCAT content spine: `<section>` is the AAMC section
//! (BB/CP/PS) and `<category>` the content-category code (1A, 4C, …). These
//! tables are a queryable, table-driven projection of that tag data: `concepts`
//! assigns each distinct concept tag a stable, append-only id, and
//! `card_concepts` maps each card to every concept tag on its note.
//!
//! Consistency with the tags is maintained by the note add/update path (which
//! calls [`SqliteStorage::refresh_card_concepts_for_note`]) and can be fully
//! reconstructed at any time via [`SqliteStorage::rebuild_concepts_from_tags`].

mod edges;
mod mastery;
mod trickle;

use std::collections::HashMap;

use rusqlite::params;

use super::SqliteStorage;
use crate::error::Result;
use crate::prelude::*;
use crate::tags::split_tags;

crate::define_newtype!(ConceptId, i64);

/// Note-tag prefix identifying a concept tag, e.g.
/// `concept::BB::1A::amino_acids`.
pub(crate) const CONCEPT_TAG_PREFIX: &str = "concept::";

/// A row of the `concepts` table.
///
/// `Concept` + [`SqliteStorage::all_concepts`] are the concept-enumeration API
/// for M2's knowledge graph (edges/mastery); exercised by tests for now.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Concept {
    pub id: ConceptId,
    pub tag: String,
    pub section: String,
}

/// The concept tag and section mapped to a single card, as read back through
/// the `card_concepts` -> `concepts` join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CardConceptTag {
    pub card_id: CardId,
    pub tag: String,
    pub section: String,
}

/// Extract the AAMC `<section>` (2nd `::` segment) from a full concept tag.
///
/// e.g. `concept::BB::1A::amino_acids` -> `BB`. Returns an empty string if the
/// tag has no section segment.
pub(crate) fn section_of_concept_tag(tag: &str) -> &str {
    tag.split("::").nth(1).unwrap_or_default()
}

/// Extract the `<category>` (3rd `::` segment) from a full concept tag.
///
/// e.g. `concept::BB::1A::amino_acids` -> `1A`. Returns an empty string if the
/// tag has no category segment.
#[allow(dead_code)] // AAMC-category accessor for read-model/graph consumers; exercised by tests
pub(crate) fn category_of_concept_tag(tag: &str) -> &str {
    tag.split("::").nth(2).unwrap_or_default()
}

/// Whether the given tag is a concept tag with a non-empty section, i.e. of the
/// canonical form `concept::<section>::<category>::<topic>` (only a non-empty
/// `<section>` is required here). Malformed tags (`concept::` alone, or
/// `concept::foo` with no trailing segment) are ignored so the projection never
/// carries a concept with an empty section.
pub(crate) fn is_concept_tag(tag: &str) -> bool {
    tag.starts_with(CONCEPT_TAG_PREFIX) && !section_of_concept_tag(tag).is_empty()
}

impl SqliteStorage {
    /// Look up a concept id by its full tag.
    pub(crate) fn get_concept_id_by_tag(&self, tag: &str) -> Result<Option<ConceptId>> {
        self.db
            .prepare_cached("SELECT id FROM concepts WHERE tag = ?")?
            .query_and_then([tag], |r| Ok(ConceptId(r.get(0)?)))?
            .next()
            .transpose()
    }

    /// Return the concept for a tag, inserting one with a fresh, stable id if
    /// it does not yet exist. Existing ids are never renumbered
    /// (append-only).
    pub(crate) fn get_or_create_concept(&self, tag: &str) -> Result<ConceptId> {
        if let Some(id) = self.get_concept_id_by_tag(tag)? {
            return Ok(id);
        }
        let section = section_of_concept_tag(tag);
        self.db
            .prepare_cached("INSERT INTO concepts (tag, section, mtime_secs) VALUES (?, ?, ?)")?
            .execute(params![tag, section, TimestampSecs::now()])?;
        Ok(ConceptId(self.db.last_insert_rowid()))
    }

    /// All concepts in the collection, ordered by tag.
    #[allow(dead_code)] // M2 knowledge-graph API; exercised by tests for now
    pub(crate) fn all_concepts(&self) -> Result<Vec<Concept>> {
        self.db
            .prepare_cached("SELECT id, tag, section FROM concepts ORDER BY tag")?
            .query_and_then([], |r| {
                Ok(Concept {
                    id: ConceptId(r.get(0)?),
                    tag: r.get(1)?,
                    section: r.get(2)?,
                })
            })?
            .collect()
    }

    /// Replace the `card_concepts` rows for the given card so they exactly
    /// match the provided concept ids.
    fn set_card_concepts(&self, card_id: CardId, concept_ids: &[ConceptId]) -> Result<()> {
        self.db
            .prepare_cached("DELETE FROM card_concepts WHERE card_id = ?")?
            .execute([card_id])?;
        let mut stmt = self.db.prepare_cached(
            "INSERT OR IGNORE INTO card_concepts (card_id, concept_id) VALUES (?, ?)",
        )?;
        for concept_id in concept_ids {
            stmt.execute(params![card_id, concept_id])?;
        }
        Ok(())
    }

    /// Upsert the concepts named by `tags` and refresh the `card_concepts` rows
    /// of every card on `note_id` to match. This is the seam the note
    /// add/update path uses to keep the projection consistent with the tags.
    ///
    /// A card belongs to every concept tag on its note; non-concept tags are
    /// ignored.
    pub(crate) fn refresh_card_concepts_for_note(
        &self,
        note_id: NoteId,
        tags: &[String],
    ) -> Result<()> {
        let mut concept_ids = Vec::new();
        for tag in tags {
            if is_concept_tag(tag) {
                concept_ids.push(self.get_or_create_concept(tag)?);
            }
        }
        concept_ids.sort_unstable();
        concept_ids.dedup();

        for card_id in self.all_card_ids_of_note_in_template_order(note_id)? {
            self.set_card_concepts(card_id, &concept_ids)?;
        }
        Ok(())
    }

    /// Reconstruct `concepts` and `card_concepts` entirely from the current
    /// note tags. Existing concept ids are preserved (append-only);
    /// `card_concepts` is rebuilt from scratch. Safe to run repeatedly, and
    /// used both by the schema migration and as a repair entry point.
    pub(crate) fn rebuild_concepts_from_tags(&self) -> Result<()> {
        // Cache tag -> concept id so we upsert each distinct tag at most once.
        let mut tag_to_id: HashMap<String, ConceptId> = HashMap::new();

        // note id -> concept ids present on that note (deduped, sorted)
        let mut note_concepts: HashMap<NoteId, Vec<ConceptId>> = HashMap::new();
        let mut stmt = self
            .db
            .prepare_cached("SELECT id, tags FROM notes WHERE tags != ''")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let note_id: NoteId = row.get(0)?;
            let raw_tags = row.get_ref_unwrap(1).as_str()?;
            let mut ids = Vec::new();
            for tag in split_tags(raw_tags) {
                if !is_concept_tag(tag) {
                    continue;
                }
                let id = match tag_to_id.get(tag) {
                    Some(id) => *id,
                    None => {
                        let id = self.get_or_create_concept(tag)?;
                        tag_to_id.insert(tag.to_string(), id);
                        id
                    }
                };
                ids.push(id);
            }
            if !ids.is_empty() {
                ids.sort_unstable();
                ids.dedup();
                note_concepts.insert(note_id, ids);
            }
        }
        drop(rows);

        self.db.execute_batch("DELETE FROM card_concepts")?;
        let mut card_stmt = self.db.prepare_cached("SELECT id, nid FROM cards")?;
        let mut insert_stmt = self.db.prepare_cached(
            "INSERT OR IGNORE INTO card_concepts (card_id, concept_id) VALUES (?, ?)",
        )?;
        let mut card_rows = card_stmt.query([])?;
        while let Some(row) = card_rows.next()? {
            let card_id: CardId = row.get(0)?;
            let note_id: NoteId = row.get(1)?;
            if let Some(ids) = note_concepts.get(&note_id) {
                for concept_id in ids {
                    insert_stmt.execute(params![card_id, concept_id])?;
                }
            }
        }
        Ok(())
    }

    /// The (card, concept) pairs for the concept-memory read model, restricted
    /// to the cards currently in the `search_cids` table. Each card appears
    /// once per distinct concept it maps to.
    pub(crate) fn card_concept_tags_in_search(&self) -> Result<Vec<CardConceptTag>> {
        self.db
            .prepare_cached(include_str!("card_concept_tags_in_search.sql"))?
            .query_and_then([], |r| {
                Ok(CardConceptTag {
                    card_id: CardId(r.get(0)?),
                    tag: r.get(1)?,
                    section: r.get(2)?,
                })
            })?
            .collect()
    }

    /// The concept ids mapped to a single card via `card_concepts`, ordered by
    /// id. Empty when the card's note carries no concept tag.
    pub(crate) fn concept_ids_for_card(&self, card_id: CardId) -> Result<Vec<ConceptId>> {
        self.db
            .prepare_cached(
                "SELECT concept_id FROM card_concepts WHERE card_id = ? ORDER BY concept_id",
            )?
            .query_and_then([card_id], |r| Ok(ConceptId(r.get(0)?)))?
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn all_card_concepts_sorted(&self) -> Result<Vec<(CardId, ConceptId)>> {
        let mut rows: Vec<(CardId, ConceptId)> = self
            .db
            .prepare("SELECT card_id, concept_id FROM card_concepts")?
            .query_and_then([], |r| Ok((CardId(r.get(0)?), ConceptId(r.get(1)?))))?
            .collect::<Result<_>>()?;
        rows.sort_unstable();
        Ok(rows)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::storage::SchemaVersion;

    #[test]
    fn section_and_tag_parsing() {
        // Canonical `concept::<section>::<category>::<topic>` form.
        assert_eq!(
            section_of_concept_tag("concept::BB::1A::amino_acids"),
            "BB"
        );
        assert_eq!(
            category_of_concept_tag("concept::BB::1A::amino_acids"),
            "1A"
        );
        assert_eq!(section_of_concept_tag("concept::CP"), "CP");
        // no category segment -> empty
        assert_eq!(category_of_concept_tag("concept::CP"), "");
        assert_eq!(section_of_concept_tag("concept::"), "");
        assert_eq!(category_of_concept_tag("concept::"), "");
        assert_eq!(section_of_concept_tag("other::x::y"), "x");
        assert_eq!(category_of_concept_tag("other::x::y"), "y");

        assert!(is_concept_tag("concept::BB::1A::amino_acids"));
        assert!(is_concept_tag("concept::CP::4A::translational_motion"));
        // section present but no trailing segments is still a usable concept
        assert!(is_concept_tag("concept::CP"));
        // malformed / non-concept tags are rejected
        assert!(!is_concept_tag("concept::"));
        assert!(!is_concept_tag("concepts::BB::1A::x"));
        assert!(!is_concept_tag("BB::amino"));
        assert!(!is_concept_tag(""));
    }

    #[test]
    fn concept_ids_are_stable_and_append_only() -> Result<()> {
        let mut col = Collection::new();
        // ver should be at the latest schema, with the concept tables present.
        assert_eq!(col.storage.db_scalar::<u8>("select ver from col")?, 22);

        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        let mut note = nt.new_note();
        note.tags = vec!["concept::BB::1A::amino_acids".into()];
        col.add_note(&mut note, DeckId(1))?;

        let first_id = col
            .storage
            .get_concept_id_by_tag("concept::BB::1A::amino_acids")?
            .unwrap();

        // create a second concept
        let mut note2 = nt.new_note();
        note2.set_field(0, "second")?;
        note2.tags = vec!["concept::CP::4A::translational_motion".into()];
        col.add_note(&mut note2, DeckId(1))?;

        // re-requesting the first tag returns the same id (never renumbered)
        assert_eq!(
            col.storage
                .get_or_create_concept("concept::BB::1A::amino_acids")?,
            first_id
        );

        // a rebuild preserves existing ids
        col.storage.rebuild_concepts_from_tags()?;
        assert_eq!(
            col.storage
                .get_concept_id_by_tag("concept::BB::1A::amino_acids")?
                .unwrap(),
            first_id
        );
        Ok(())
    }

    #[test]
    fn add_note_populates_card_concepts() -> Result<()> {
        let mut col = Collection::new();
        // The schema-20 seed pre-creates some concept rows on open; measure
        // against that baseline rather than assuming an empty table.
        let baseline_concepts = col.storage.all_concepts()?.len();
        let nt = col
            .get_notetype_by_name("basic (and reversed card)")?
            .unwrap();
        let mut note = nt.new_note();
        note.set_field(0, "front")?;
        note.set_field(1, "back")?;
        // Use a spine topic NOT referenced by any seed prerequisite edge, so the
        // schema-22 seed load has not already created it: adding this note must
        // create exactly one new concept row.
        note.tags = vec![
            "concept::BB::1A::nonenzymatic_protein_function".into(),
            "unrelated".into(),
        ];
        col.add_note(&mut note, DeckId(1))?;

        // basic+reversed generates two cards; each maps to the single concept.
        let concept_id = col
            .storage
            .get_concept_id_by_tag("concept::BB::1A::nonenzymatic_protein_function")?
            .unwrap();
        let card_ids = col
            .storage
            .all_card_ids_of_note_in_template_order(note.id)?;
        assert_eq!(card_ids.len(), 2);
        let mut expected: Vec<(CardId, ConceptId)> =
            card_ids.iter().map(|cid| (*cid, concept_id)).collect();
        expected.sort_unstable();
        assert_eq!(col.storage.all_card_concepts_sorted()?, expected);

        // the note added exactly one new concept
        // ("concept::BB::1A::nonenzymatic_protein_function"); the non-concept
        // "unrelated" tag must not create a concept row.
        assert_eq!(col.storage.all_concepts()?.len(), baseline_concepts + 1);
        assert!(col.storage.get_concept_id_by_tag("unrelated")?.is_none());

        Ok(())
    }

    #[test]
    fn updating_tags_refreshes_card_concepts() -> Result<()> {
        let mut col = Collection::new();
        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        let mut note = nt.new_note();
        note.set_field(0, "front")?;
        note.tags = vec!["concept::BB::1A::amino_acids".into()];
        col.add_note(&mut note, DeckId(1))?;
        let cid = col
            .storage
            .all_card_ids_of_note_in_template_order(note.id)?[0];
        let amino = col
            .storage
            .get_concept_id_by_tag("concept::BB::1A::amino_acids")?
            .unwrap();
        assert_eq!(col.storage.all_card_concepts_sorted()?, vec![(cid, amino)]);

        // swap the concept tag for a different one
        note.tags = vec!["concept::CP::4A::translational_motion".into()];
        col.update_note(&mut note)?;
        let kin = col
            .storage
            .get_concept_id_by_tag("concept::CP::4A::translational_motion")?
            .unwrap();
        assert_eq!(col.storage.all_card_concepts_sorted()?, vec![(cid, kin)]);

        // removing all concept tags clears the card's rows
        note.tags = vec![];
        col.update_note(&mut note)?;
        assert!(col.storage.all_card_concepts_sorted()?.is_empty());

        Ok(())
    }

    #[test]
    fn migration_rebuilds_projection_from_tags() -> Result<()> {
        use anki_io::new_tempfile;

        use crate::collection::CollectionBuilder;

        let tempfile = new_tempfile()?;
        let mut col = CollectionBuilder::default()
            .set_collection_path(tempfile.path())
            .build()?;
        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        let mut note = nt.new_note();
        note.set_field(0, "front")?;
        note.tags = vec!["concept::BB::1A::amino_acids".into()];
        col.add_note(&mut note, DeckId(1))?;
        let cid = col
            .storage
            .all_card_ids_of_note_in_template_order(note.id)?[0];
        let before = col.storage.all_card_concepts_sorted()?;
        assert_eq!(before.len(), 1);

        // Closing to schema 18 drops the local concept projection...
        col.close(Some(SchemaVersion::V18))?;
        let col = CollectionBuilder::default()
            .set_collection_path(tempfile.path())
            .build()?;
        // ...and reopening runs the schema 19 + 20 + 21 + 22 migrations, which
        // reconstruct it from the surviving `concept::` note tags.
        assert_eq!(col.storage.db_scalar::<u8>("select ver from col")?, 22);
        let after = col.storage.all_card_concepts_sorted()?;
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].0, cid);
        assert_eq!(
            col.storage
                .get_concept_id_by_tag("concept::BB::1A::amino_acids")?
                .map(|_| ()),
            Some(())
        );
        Ok(())
    }
}
