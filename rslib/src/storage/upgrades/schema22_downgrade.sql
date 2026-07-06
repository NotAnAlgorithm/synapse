-- Synapse: undo the schema22 spine re-key when downgrading toward the
-- schema-18 sync/colpkg format. Schema22 changed no table shape — it only
-- truncated + rebuilt the LOCAL, DERIVED concept projection and stepped the
-- version to 22 — so there is nothing to drop here; the subsequent
-- schema21/20/19 downgrades drop the tables themselves. This step only returns
-- the schema version to 21, which the drop chain then steps down to 18.
UPDATE col
SET ver = 21;