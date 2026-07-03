-- Synapse (M2, workstream B): drop the local, DERIVED card-lineage projection
-- when downgrading to the schema-18 sync/colpkg format. The table is local and
-- rebuilds from each card's custom_data {"src": <nid>} on the next open, so it
-- must not leak into the synced/exported schema-18 collection. Mirrors the
-- concept-table downgrade policy: local derived tables are dropped, not synced.
DROP TABLE IF EXISTS card_lineage;
-- Return the schema version to the wire/on-disk format (18). The V11 downgrade
-- chains through this first, then schema18_downgrade.sql steps it down further.
UPDATE col
SET ver = 18;
