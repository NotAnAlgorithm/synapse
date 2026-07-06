-- Synapse: undo the schema24 concept-projection + prerequisite-graph rebuild
-- when downgrading toward the schema-18 sync/colpkg format. Schema24 changed no
-- table shape — it only re-seeded card_concepts + concept_edges and stepped the
-- version to 24 — so there is nothing to drop here; the subsequent
-- schema23/22/21/20/19 downgrades drop the tables themselves. This step only
-- returns the schema version to 23.
UPDATE col
SET ver = 23;