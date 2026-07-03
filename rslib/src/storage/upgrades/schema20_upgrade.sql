-- Synapse prerequisite knowledge graph over concepts.
--
-- Like the `concepts`/`card_concepts` projection introduced in schema 19, this
-- table is a LOCAL, DERIVED structure that is never synced. Its rows reference
-- `concepts.id`, which are themselves local/derived (assigned append-only from
-- the `concept::` note tags). Authored seed edges are supplied as tag pairs and
-- resolved to ids by the schema 20 upgrade step (and by the rebuild-from-seed
-- entry point), so the table can always be reconstructed from the seed + tags.
--
-- Direction: a row (from_concept_id, to_concept_id) means "from is a
-- PREREQUISITE of to" — i.e. you should master `from` before `to`. Equivalently,
-- `to` DEPENDS ON `from`.
CREATE TABLE concept_edges (
  -- The prerequisite concept.
  from_concept_id integer NOT NULL,
  -- The dependent concept (requires the prerequisite).
  to_concept_id integer NOT NULL,
  mtime_secs integer NOT NULL,
  PRIMARY KEY (from_concept_id, to_concept_id)
) WITHOUT ROWID;
-- Look up a concept's dependents (rows where it is the prerequisite) directly;
-- the primary key already covers prerequisite lookups (rows where it is the
-- dependent) via the leading column.
CREATE INDEX idx_concept_edges_to ON concept_edges (to_concept_id);
UPDATE col
SET ver = 20;
