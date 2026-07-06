-- Synapse: rebuild the LOCAL, DERIVED concept projection AND prerequisite graph
-- from the current note tags + the vendored spine.
--
-- The text/CSV importer adds notes through the low-level path
-- (add_note_only_undoable), which bypasses the per-note concept-projection
-- refresh the interactive add path performs. So a collection that imported the
-- demo cards after an earlier migration ran ended up with an empty
-- `card_concepts` projection — the concept graph/coverage/dashboard showed no
-- nodes even though the cards carry `concept::` tags. This repairs them.
--
-- No table shape change: `card_concepts` is truncated + rebuilt by
-- rebuild_concepts_from_tags() and `concept_edges` by
-- rebuild_concept_edges_from_seed(), both run immediately after this SQL.
-- `concepts` ids are append-only and preserved.
DELETE FROM concept_edges;
UPDATE col
SET ver = 24;