-- Synapse: undo the schema23 prerequisite-graph refresh when downgrading toward
-- the schema-18 sync/colpkg format. Schema23 changed no table shape — it only
-- re-seeded concept_edges and stepped the version to 23 — so there is nothing
-- to drop here; the subsequent schema22/21/20/19 downgrades drop the tables
-- themselves. This step only returns the schema version to 22.
UPDATE col
SET ver = 22;