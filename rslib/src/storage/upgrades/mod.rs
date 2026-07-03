// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

/// The minimum schema version we can open.
pub(super) const SCHEMA_MIN_VERSION: u8 = 11;
/// The version new files are initially created with.
pub(super) const SCHEMA_STARTING_VERSION: u8 = 11;
/// The maximum schema version we can open.
///
/// Synapse (M2): bumped past the wire/on-disk format (18) to accommodate LOCAL,
/// DERIVED tables that never sync. Schema 21 adds the card-lineage projection
/// (workstream B). These local tables are dropped on downgrade to the
/// schema-18 sync/colpkg format and rebuilt on next open.
///
/// NOTE FOR INTEGRATOR: on the integrated branch, workstreams A (concepts, sch.
/// 19/20) and B (lineage, sch. 21) both bump this constant. Reconcile so that
/// SCHEMA_MAX_VERSION == 21 and the per-version upgrade blocks below run in
/// ascending order (19 → 20 → 21). This base branch (M0-only) has no 19/20
/// blocks, so opening a schema-18 file jumps straight to 21 running only the
/// lineage block; that is intentional and still correct.
pub(super) const SCHEMA_MAX_VERSION: u8 = 21;

use super::SchemaVersion;
use super::SqliteStorage;
use crate::error::Result;

impl SqliteStorage {
    pub(super) fn upgrade_to_latest_schema(&self, ver: u8, server: bool) -> Result<()> {
        if ver < 14 {
            self.db
                .execute_batch(include_str!("schema14_upgrade.sql"))?;
            self.upgrade_deck_conf_to_schema14()?;
            self.upgrade_tags_to_schema14()?;
            self.upgrade_config_to_schema14()?;
        }
        if ver < 15 {
            self.db
                .execute_batch(include_str!("schema15_upgrade.sql"))?;
            self.upgrade_notetypes_to_schema15()?;
            self.upgrade_decks_to_schema15(server)?;
            self.upgrade_deck_conf_to_schema15()?;
        }
        if ver < 16 {
            self.upgrade_deck_conf_to_schema16(server)?;
            self.db.execute_batch("update col set ver = 16")?;
        }
        if ver < 17 {
            self.upgrade_tags_to_schema17()?;
            self.db.execute_batch("update col set ver = 17")?;
        }
        if ver < 18 {
            self.db
                .execute_batch(include_str!("schema18_upgrade.sql"))?;
        }
        // Synapse (M2, workstream B): local, derived card-lineage projection.
        // Creates the table (schema21_upgrade.sql) and backfills it from
        // existing cards' custom_data.src. INTEGRATOR: workstream A's schema
        // 19/20 blocks belong immediately above this one, in order.
        if ver < 21 {
            self.db
                .execute_batch(include_str!("schema21_upgrade.sql"))?;
            self.upgrade_lineage_to_schema21()?;
        }

        // in some future schema upgrade, we may want to change
        // _collapsed to _expanded in DeckCommon and invert existing values, so
        // that we can avoid serializing the values in the default case, and use
        // DeckCommon::default() in new_normal() and new_filtered()

        Ok(())
    }

    pub(super) fn downgrade_to(&self, ver: SchemaVersion) -> Result<()> {
        match ver {
            SchemaVersion::V11 => self.downgrade_to_schema_11(),
            SchemaVersion::V18 => {
                // Both sync-eligible target formats are schema 18. Drop the
                // local, derived Synapse tables so they never enter the
                // synced/exported collection; they rebuild from source on the
                // next open. (V11 chains through this via its schema-18 step.)
                self.begin_trx()?;
                self.drop_local_synapse_tables()?;
                self.commit_trx()?;
                Ok(())
            }
        }
    }

    /// Drop the LOCAL, DERIVED Synapse tables that must not appear in the
    /// schema-18 wire/on-disk format. INTEGRATOR: workstream A's concept-table
    /// drop belongs here too (both run before an upload/export).
    fn drop_local_synapse_tables(&self) -> Result<()> {
        self.db
            .execute_batch(include_str!("schema21_downgrade.sql"))?;
        Ok(())
    }

    fn downgrade_to_schema_11(&self) -> Result<()> {
        self.begin_trx()?;

        self.drop_local_synapse_tables()?;
        self.db
            .execute_batch(include_str!("schema18_downgrade.sql"))?;
        self.downgrade_deck_conf_from_schema16()?;
        self.downgrade_decks_from_schema15()?;
        self.downgrade_notetypes_from_schema15()?;
        self.downgrade_config_from_schema14()?;
        self.downgrade_tags_from_schema14()?;
        self.db
            .execute_batch(include_str!("schema11_downgrade.sql"))?;

        self.commit_trx()?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use anki_io::new_tempfile;

    use super::*;
    use crate::collection::CollectionBuilder;
    use crate::prelude::*;

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn assert_latest_schema_version() {
        // The wire/on-disk format is still schema 18; local Synapse tables push
        // the openable max to 21 (see SCHEMA_MAX_VERSION). If this bumps again,
        // ensure downgrade_to(SchemaVersion::V18) drops any new local table.
        assert_eq!(
            21, SCHEMA_MAX_VERSION,
            "on bump, update downgrade_to() to drop new local tables and keep V18 the sync format"
        );
    }

    #[test]
    fn valid_ease_factor_survives_upgrade_roundtrip() -> Result<()> {
        let tempfile = new_tempfile()?;
        let mut col = CollectionBuilder::default()
            .set_collection_path(tempfile.path())
            .build()?;
        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        let mut note = nt.new_note();
        col.add_note(&mut note, DeckId(1))?;
        col.storage
            .db
            .execute("update cards set factor = 1400", [])?;
        col.close(Some(SchemaVersion::V11))?;
        let col = CollectionBuilder::default()
            .set_collection_path(tempfile.path())
            .build()?;
        let card = &col.storage.get_all_cards()[0];
        assert_eq!(card.ease_factor, 1400);
        Ok(())
    }

    /// Synapse (M2, workstream B): the schema21 upgrade backfills card_lineage
    /// from existing cards' custom_data.src, and the schema-18 downgrade drops
    /// the local table (rebuilt on reopen). Exercises the full roundtrip.
    #[test]
    fn lineage_migration_roundtrip() -> Result<()> {
        use crate::card::CardId;
        use crate::notes::NoteId;

        let tempfile = new_tempfile()?;
        let mut col = CollectionBuilder::default()
            .set_collection_path(tempfile.path())
            .build()?;
        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        let mut note = nt.new_note();
        col.add_note(&mut note, DeckId(1))?;
        let card_id: CardId = col.storage.get_all_cards()[0].id;

        // Write custom_data.src directly (as an older collection would have,
        // before the mirror hook existed) and blow away the derived table to
        // simulate a pre-schema-21 file.
        col.storage.db.execute(
            r#"update cards set data = '{"cd":"{\"src\":555}"}' where id = ?"#,
            [card_id],
        )?;
        col.storage.db.execute("DROP TABLE card_lineage", [])?;

        // Close to the wire format (drops the local table if present, sets
        // ver=18) then reopen, which upgrades to 21 and backfills.
        col.close(Some(SchemaVersion::V18))?;
        let col = CollectionBuilder::default()
            .set_collection_path(tempfile.path())
            .build()?;

        let row = col.card_lineage(card_id)?.expect("lineage backfilled");
        assert_eq!(row.source_note_id, NoteId(555));

        // And the downgrade actually removes the local table from the on-disk
        // schema-18 form.
        col.close(Some(SchemaVersion::V18))?;
        let col = CollectionBuilder::default()
            .set_collection_path(tempfile.path())
            .build()?;
        // Table is recreated + rebuilt on reopen, so the row is back.
        assert!(col.card_lineage(card_id)?.is_some());
        Ok(())
    }
}
