-- Synapse (M2, workstream B): local, DERIVED card-lineage projection.
--
-- Mirrors a minted card back to the source note it was created from. This is a
-- queryable superset of the ~100-byte custom_data {"src": <nid>} lineage that
-- M0's mint.py stamps on each generated card. The table is LOCAL and DERIVED:
-- it is never synced and is not part of the schema-18 wire/on-disk format. The
-- schema21 downgrade (full-sync upload / colpkg export) DROPS it, and it is
-- rebuilt from custom_data on next open (see rebuild_card_lineage_from_data).
--
-- Row semantics: one row per minted card. `card_id` is the generated card,
-- `source_note_id` is the note it was minted from, `relation` labels the edge
-- (currently always 'minted_from'; kept for future lineage kinds).
CREATE TABLE IF NOT EXISTS card_lineage (
  card_id integer PRIMARY KEY NOT NULL,
  source_note_id integer NOT NULL,
  relation text NOT NULL
) WITHOUT ROWID;
-- Reverse lookup: "which cards were minted from this note?" (lineage_for_source_note).
CREATE INDEX IF NOT EXISTS idx_card_lineage_source ON card_lineage (source_note_id);
UPDATE col
SET ver = 21;
