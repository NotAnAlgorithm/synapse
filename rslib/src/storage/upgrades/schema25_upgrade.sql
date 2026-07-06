-- Synapse: re-seed the LOCAL, DERIVED prerequisite graph after the AAMC spine
-- was edited to connect the previously-isolated central-nervous-system topic
-- (and to give collections that already migrated a seam that picks up later
-- spine-edge edits). No table shape change: `concept_edges` keeps its schema-20
-- definition; it is truncated here and rebuilt from the vendored spine seed by
-- rebuild_concept_edges_from_seed(), which the schema25 upgrade step runs
-- immediately after this SQL. `concepts`/`card_concepts` are left untouched —
-- they derive from note tags and are unaffected by the edge change.
DELETE FROM concept_edges;
UPDATE col
SET ver = 25;
