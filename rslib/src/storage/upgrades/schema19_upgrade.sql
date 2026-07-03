-- Synapse concept layer, derived from `concept::<section>::<id>` note tags.
-- The tags remain the source of truth; these tables are a queryable projection
-- kept in sync by the note add/update path and rebuildable from the tags.
CREATE TABLE concepts (
  -- Stable, append-only id. Never renumbered once assigned to a tag.
  id integer NOT NULL PRIMARY KEY,
  -- Full concept tag, e.g. `concept::biochem::amino_acid_charge`.
  tag text NOT NULL UNIQUE,
  -- The `<section>` segment (2nd `::` segment) of the tag.
  section text NOT NULL,
  mtime_secs integer NOT NULL
);
CREATE TABLE card_concepts (
  card_id integer NOT NULL,
  concept_id integer NOT NULL,
  PRIMARY KEY (card_id, concept_id)
) WITHOUT ROWID;
CREATE INDEX idx_card_concepts_concept ON card_concepts (concept_id);
UPDATE col
SET ver = 19;
