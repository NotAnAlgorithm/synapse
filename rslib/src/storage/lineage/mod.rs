// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Synapse (M2, workstream B): the card-lineage projection.
//!
//! A minted recall card is linked back to the source note it was created from.
//! M0's `mint.py` stamps that link on the generated card as a ~100-byte
//! `custom_data` blob (`{"src": <source_note_id>}`); this table lifts it into a
//! queryable, indexed form ("which cards were minted from this note?") without
//! changing where the link is *authored*. `custom_data.src` stays the write
//! source; the table is a DERIVED mirror kept current on the card-save path.
//!
//! Sync/persistence policy (mirrors the concept tables): the table is LOCAL and
//! DERIVED. It is not part of the schema-18 wire/on-disk format, is never
//! synced, and is dropped by the schema21 downgrade (full-sync upload / colpkg
//! export), rebuilding from `custom_data` on next open.

use rusqlite::params;
use rusqlite::OptionalExtension;

use super::SqliteStorage;
use crate::prelude::*;

/// The `custom_data` key `mint.py` writes with the source note id.
pub(crate) const LINEAGE_SRC_KEY: &str = "src";
/// The only relation kind emitted today: a card minted from a source note.
/// Stored as text so future lineage kinds can be added without a migration.
pub(crate) const RELATION_MINTED_FROM: &str = "minted_from";

/// One card-lineage edge: the minted `card_id` came from `source_note_id` via
/// `relation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardLineage {
    pub card_id: CardId,
    pub source_note_id: NoteId,
    pub relation: String,
}

impl SqliteStorage {
    /// Create + populate the local card-lineage table when upgrading a
    /// pre-schema-21 collection. The table itself is created by
    /// `schema21_upgrade.sql`; this backfills it from existing cards'
    /// `custom_data.src`.
    pub(super) fn upgrade_lineage_to_schema21(&self) -> Result<()> {
        self.rebuild_card_lineage_from_data()
    }

    /// Rebuild the entire card-lineage table from every card's
    /// `custom_data.src`. Repair/regeneration entry point: safe to call at any
    /// time (used by the migration and exposed via
    /// `Collection::rebuild_card_lineage`). Uses the `extract_custom_data`
    /// scalar function registered at db-open time.
    pub(crate) fn rebuild_card_lineage_from_data(&self) -> Result<()> {
        self.db.execute("DELETE FROM card_lineage", [])?;
        self.db.execute(
            "INSERT OR REPLACE INTO card_lineage (card_id, source_note_id, relation)
             SELECT id,
                    CAST(extract_custom_data(data, ?1) AS integer),
                    ?2
             FROM cards
             WHERE extract_custom_data(data, ?1) IS NOT NULL
               AND CAST(extract_custom_data(data, ?1) AS integer) > 0",
            params![LINEAGE_SRC_KEY, RELATION_MINTED_FROM],
        )?;
        Ok(())
    }

    /// Upsert the lineage row for a single card, or clear it, to match the
    /// card's current `custom_data.src`. Called from the card-save path so the
    /// projection tracks `custom_data` writes (including mint.py's stamp and
    /// any later clearing of the `src` key). No-op-equivalent: writing the
    /// same value twice is idempotent.
    pub(crate) fn mirror_card_lineage_from_data(&self, card: &Card) -> Result<()> {
        match source_note_id_from_custom_data(&card.custom_data) {
            Some(source_note_id) => {
                self.set_card_lineage(card.id, source_note_id, RELATION_MINTED_FROM)
            }
            None => self.remove_card_lineage(card.id),
        }
    }

    /// Insert or replace the lineage row for `card_id`.
    pub(crate) fn set_card_lineage(
        &self,
        card_id: CardId,
        source_note_id: NoteId,
        relation: &str,
    ) -> Result<()> {
        self.db
            .prepare_cached(
                "INSERT OR REPLACE INTO card_lineage (card_id, source_note_id, relation)
                 VALUES (?, ?, ?)",
            )?
            .execute(params![card_id, source_note_id, relation])?;
        Ok(())
    }

    /// Fetch the lineage row for `card_id`, if any.
    pub(crate) fn get_card_lineage(&self, card_id: CardId) -> Result<Option<CardLineage>> {
        self.db
            .prepare_cached(
                "SELECT card_id, source_note_id, relation FROM card_lineage WHERE card_id = ?",
            )?
            .query_row([card_id], row_to_card_lineage)
            .optional()
            .map_err(Into::into)
    }

    /// Remove the lineage row for `card_id` (no-op if absent).
    pub(crate) fn remove_card_lineage(&self, card_id: CardId) -> Result<()> {
        self.db
            .prepare_cached("DELETE FROM card_lineage WHERE card_id = ?")?
            .execute([card_id])?;
        Ok(())
    }

    /// All cards minted from `source_note_id`, ordered by card id for
    /// determinism.
    pub(crate) fn lineage_for_source_note(
        &self,
        source_note_id: NoteId,
    ) -> Result<Vec<CardLineage>> {
        self.db
            .prepare_cached(
                "SELECT card_id, source_note_id, relation
                 FROM card_lineage WHERE source_note_id = ? ORDER BY card_id",
            )?
            .query_and_then([source_note_id], |r| {
                row_to_card_lineage(r).map_err(Into::into)
            })?
            .collect()
    }
}

impl Collection {
    /// Repair/regenerate the entire card-lineage projection from every card's
    /// `custom_data.src`. Runs inside a transaction. Exposed for the integrator
    /// (e.g. a maintenance/db-check action) and mirrors the migration backfill.
    pub fn rebuild_card_lineage(&mut self) -> Result<()> {
        self.transact_no_undo(|col| col.storage.rebuild_card_lineage_from_data())
    }

    /// Fetch the lineage row for a card, if any.
    pub fn card_lineage(&self, card_id: CardId) -> Result<Option<CardLineage>> {
        self.storage.get_card_lineage(card_id)
    }

    /// All cards minted from `source_note_id`.
    pub fn card_lineage_for_source_note(&self, source_note_id: NoteId) -> Result<Vec<CardLineage>> {
        self.storage.lineage_for_source_note(source_note_id)
    }
}

fn row_to_card_lineage(row: &rusqlite::Row) -> rusqlite::Result<CardLineage> {
    Ok(CardLineage {
        card_id: CardId(row.get(0)?),
        source_note_id: NoteId(row.get(1)?),
        relation: row.get(2)?,
    })
}

/// Parse the source note id out of a card's `custom_data` JSON, if the `src`
/// key is present and holds a positive integer. Tolerant of empty/invalid
/// data (returns `None`), matching the `extract_custom_data` scalar function.
pub(crate) fn source_note_id_from_custom_data(custom_data: &str) -> Option<NoteId> {
    if custom_data.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(custom_data).ok()?;
    let src = value.get(LINEAGE_SRC_KEY)?;
    // mint.py writes a JSON number; tolerate a stringified number too.
    let id = match src {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    }?;
    (id > 0).then_some(NoteId(id))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_src() {
        assert_eq!(source_note_id_from_custom_data(""), None);
        assert_eq!(source_note_id_from_custom_data("{}"), None);
        assert_eq!(
            source_note_id_from_custom_data(r#"{"src":123}"#),
            Some(NoteId(123))
        );
        // stringified number tolerated
        assert_eq!(
            source_note_id_from_custom_data(r#"{"src":"456"}"#),
            Some(NoteId(456))
        );
        // other keys ignored, invalid/zero rejected
        assert_eq!(source_note_id_from_custom_data(r#"{"other":1}"#), None);
        assert_eq!(source_note_id_from_custom_data(r#"{"src":0}"#), None);
        assert_eq!(source_note_id_from_custom_data(r#"{"src":-1}"#), None);
        assert_eq!(source_note_id_from_custom_data("not json"), None);
    }

    /// The card-save hook upserts a lineage row when a card gains
    /// custom_data.src, and clears it when the src is removed.
    #[test]
    fn save_hook_mirrors_lineage() {
        let mut col = Collection::new();
        let note = NoteAdder::basic(&mut col).add(&mut col);
        let card = col.storage.all_cards_of_note(note.id).unwrap()[0].clone();

        // No lineage initially.
        assert_eq!(col.card_lineage(card.id).unwrap(), None);

        // Stamp custom_data.src and save -> lineage row appears.
        col.get_and_update_card(card.id, |c| {
            c.custom_data = r#"{"src":4242}"#.to_string();
            Ok(())
        })
        .unwrap();
        let row = col.card_lineage(card.id).unwrap().unwrap();
        assert_eq!(row.source_note_id, NoteId(4242));
        assert_eq!(row.relation, RELATION_MINTED_FROM);
        // Reverse lookup works.
        assert_eq!(
            col.card_lineage_for_source_note(NoteId(4242)).unwrap(),
            vec![row]
        );

        // Clearing src on a later save removes the lineage row.
        col.get_and_update_card(card.id, |c| {
            c.custom_data = String::new();
            Ok(())
        })
        .unwrap();
        assert_eq!(col.card_lineage(card.id).unwrap(), None);
    }

    /// A repair/migration rebuild reconstructs the table purely from
    /// custom_data.src, even if the table was emptied out of band.
    #[test]
    fn rebuild_from_custom_data() {
        let mut col = Collection::new();
        let note = NoteAdder::basic(&mut col).add(&mut col);
        let card = col.storage.all_cards_of_note(note.id).unwrap()[0].clone();
        col.get_and_update_card(card.id, |c| {
            c.custom_data = r#"{"src":777}"#.to_string();
            Ok(())
        })
        .unwrap();
        assert!(col.card_lineage(card.id).unwrap().is_some());

        // Simulate a stale/emptied projection (as if just after the schema21
        // table create, before backfill).
        col.storage
            .db
            .execute("DELETE FROM card_lineage", [])
            .unwrap();
        assert_eq!(col.card_lineage(card.id).unwrap(), None);

        // Rebuild reconstructs it from the card's custom_data.
        col.rebuild_card_lineage().unwrap();
        let row = col.card_lineage(card.id).unwrap().unwrap();
        assert_eq!(row.source_note_id, NoteId(777));
    }

    /// Removing a card drops its lineage row.
    #[test]
    fn remove_card_clears_lineage() {
        let mut col = Collection::new();
        let note = NoteAdder::basic(&mut col).add(&mut col);
        let card = col.storage.all_cards_of_note(note.id).unwrap()[0].clone();
        col.storage
            .set_card_lineage(card.id, NoteId(9), RELATION_MINTED_FROM)
            .unwrap();
        assert!(col.card_lineage(card.id).unwrap().is_some());
        col.storage.remove_card(card.id).unwrap();
        assert_eq!(col.card_lineage(card.id).unwrap(), None);
    }
}
