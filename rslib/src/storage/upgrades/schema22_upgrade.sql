-- Synapse: re-key the LOCAL, DERIVED concept projection onto the AAMC MCAT
-- spine's canonical `concept::<section>::<category>::<topic>` tags.
--
-- No table shape changes here: `concepts`, `card_concepts` and `concept_edges`
-- keep their schema-19/20 definitions. The old rows were keyed to the retired
-- discipline-style tags (`concept::biochem::…`), so they are truncated and then
-- rebuilt from the current `concept::` note tags and the vendored spine seed by
-- rebuild_concepts_from_tags() + rebuild_concept_edges_from_seed(), which the
-- schema22 upgrade step runs immediately after this SQL. The tags remain the
-- source of truth, so this is a safe re-key.
DELETE FROM card_concepts;
DELETE FROM concept_edges;
DELETE FROM concepts;
UPDATE col
SET ver = 22;
