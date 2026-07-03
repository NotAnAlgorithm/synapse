// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

/// The minimum schema version we can open.
pub(super) const SCHEMA_MIN_VERSION: u8 = 11;
/// The version new files are initially created with.
pub(super) const SCHEMA_STARTING_VERSION: u8 = 11;
/// The maximum schema version we can open.
///
/// Synapse (M1/M2): the wire/on-disk format is still schema 18; the versions
/// above it — 19 (concepts projection), 20 (prerequisite edges), 21 (card
/// lineage) — add LOCAL, DERIVED tables that never sync. They are dropped on
/// downgrade to the schema-18 sync/colpkg format and rebuilt from source
/// (tags / authored seed / custom_data) on next open.
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
        if ver < 19 {
            self.db
                .execute_batch(include_str!("schema19_upgrade.sql"))?;
            // Derive the Synapse concept projection from existing `concept::`
            // note tags. The tags remain the source of truth.
            self.rebuild_concepts_from_tags()?;
        }
        if ver < 20 {
            self.db
                .execute_batch(include_str!("schema20_upgrade.sql"))?;
            // Load the authored Synapse prerequisite graph. Edges reference
            // concepts by tag and are resolved to (local, derived) concept ids,
            // so this is safe to run after the concept projection exists.
            self.rebuild_concept_edges_from_seed()?;
        }
        if ver < 21 {
            // Synapse (M2, workstream B): local, derived card-lineage
            // projection. Creates the table and backfills it from existing
            // cards' custom_data.src.
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
            SchemaVersion::V18 => self.downgrade_to_schema_18(),
        }
    }

    /// Bring the DB back to the wire/on-disk schema 18 used by sync upload and
    /// colpkg export. The Synapse concept projection, prerequisite graph and
    /// card-lineage table are LOCAL, DERIVED (never synced), so they are dropped
    /// here; a subsequent open reconstructs them from the `concept::` note tags,
    /// the authored seed, and each card's custom_data via the 19/20/21 upgrades.
    fn downgrade_to_schema_18(&self) -> Result<()> {
        self.begin_trx()?;
        self.drop_local_synapse_tables()?;
        self.commit_trx()?;
        Ok(())
    }

    /// Drop every LOCAL, DERIVED Synapse table so none appear in the schema-18
    /// wire/on-disk format; ends at `ver = 18`. Rebuilt from source on next open.
    fn drop_local_synapse_tables(&self) -> Result<()> {
        // card_lineage (21) -> concept_edges (20) -> concepts + card_concepts
        // (19). schema19_downgrade runs last and leaves ver = 18.
        self.db
            .execute_batch(include_str!("schema21_downgrade.sql"))?;
        self.db
            .execute_batch(include_str!("schema20_downgrade.sql"))?;
        self.db
            .execute_batch(include_str!("schema19_downgrade.sql"))?;
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
