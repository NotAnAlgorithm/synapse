-- Synapse: refresh the LOCAL, DERIVED prerequisite graph after the AAMC spine
-- gained its full authored `prerequisites` set (a handful of seed edges -> the
-- complete 161-topic concept graph). No table shape change: `concept_edges`
-- keeps its schema-20 definition; it is truncated here and rebuilt from the
-- vendored spine seed by rebuild_concept_edges_from_seed(), which the schema23
-- upgrade step runs immediately after this SQL. `concepts`/`card_concepts` are
-- left untouched — they derive from note tags and are unaffected by the edge
-- growth.
DELETE FROM concept_edges;
UPDATE col
SET ver = 23;