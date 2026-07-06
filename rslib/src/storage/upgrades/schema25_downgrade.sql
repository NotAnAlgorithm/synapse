-- Synapse: undo the schema25 prerequisite-graph re-seed when downgrading toward
-- the schema-18 sync/colpkg format. Schema25 changed no table shape — it only
-- re-seeded concept_edges and stepped the version to 25 — so there is nothing to
-- drop here; the subsequent schema24/23/22/21/20/19 downgrades drop the tables
-- themselves. This step only returns the schema version to 24.
UPDATE col
SET ver = 24;
