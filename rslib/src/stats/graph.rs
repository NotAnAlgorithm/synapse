// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Concept-graph read-model (PRD D1 / W3).
//!
//! Builds the data behind the Synapse concept-graph visualization: a node-link
//! view of the M2 prerequisite graph. NODES are concepts (reusing the
//! per-concept "Memory" read-model, [`Collection::concept_memory`]) coloured by
//! mastery; EDGES are the directed prerequisite links from the derived
//! `concept_edges` table (`from` is a prerequisite of `to`).
//!
//! `search` scopes the NODES only (e.g. `deck:Synapse`, empty = whole
//! collection). The prerequisite graph itself is global, so an edge is emitted
//! only when BOTH of its endpoints survived the node scope — this keeps the
//! rendered graph self-consistent (no dangling edges to concepts the scope
//! excluded) while still letting a scoped view show the relationships among the
//! concepts it contains.

use std::collections::HashMap;
use std::collections::HashSet;

use anki_proto::stats::concept_graph_response::Edge;
use anki_proto::stats::concept_graph_response::Node;
use anki_proto::stats::ConceptGraphResponse;

use crate::prelude::*;

impl Collection {
    /// The concept prerequisite graph as drawable nodes + edges (PRD D1 / W3).
    ///
    /// Nodes come from [`Collection::concept_memory`] (so they carry the same
    /// mastery signal as the Memory dashboard) scoped by `search`; edges come
    /// from the `concept_edges` table, resolved from concept ids back to tags
    /// and filtered to those whose endpoints are both present as nodes.
    pub(crate) fn concept_graph(&mut self, search: &str) -> Result<ConceptGraphResponse> {
        // Nodes: reuse the concept-memory aggregation (respects `search`).
        let memory = self.concept_memory(search)?;
        let nodes: Vec<Node> = memory
            .concepts
            .into_iter()
            .map(|c| Node {
                concept: c.concept,
                section: c.section,
                memory: c.memory,
                card_count: c.card_count,
                scored_card_count: c.scored_card_count,
                sufficient_data: c.sufficient_data,
            })
            .collect();

        // Only edges whose endpoints are both visible nodes are drawable.
        let node_tags: HashSet<&str> = nodes.iter().map(|n| n.concept.as_str()).collect();

        // Resolve concept ids -> tags once so edge rows can be rendered by tag.
        // (Concept ids are local/derived; the tag is the stable identity the
        // rest of the Synapse layer keys on.) Keyed by the raw i64 id so this
        // module doesn't need to name storage's private `ConceptId` newtype.
        let id_to_tag: HashMap<i64, String> = self
            .storage
            .all_concepts()?
            .into_iter()
            .map(|c| (c.id.0, c.tag))
            .collect();

        let mut edges: Vec<Edge> = Vec::new();
        for edge in self.storage.all_concept_edges()? {
            let (Some(from_tag), Some(to_tag)) =
                (id_to_tag.get(&edge.from.0), id_to_tag.get(&edge.to.0))
            else {
                // An edge referencing a concept id with no tag row is a broken
                // projection; skip it rather than emit a dangling edge.
                continue;
            };
            if node_tags.contains(from_tag.as_str()) && node_tags.contains(to_tag.as_str()) {
                edges.push(Edge {
                    from_concept: from_tag.clone(),
                    to_concept: to_tag.clone(),
                });
            }
        }
        // `all_concept_edges` already orders by (from, to); resolving to tags
        // preserves that pairing but not tag order, so sort for a deterministic
        // response.
        edges.sort_by(|a, b| {
            a.from_concept
                .cmp(&b.from_concept)
                .then_with(|| a.to_concept.cmp(&b.to_concept))
        });

        Ok(ConceptGraphResponse { nodes, edges })
    }
}

#[cfg(test)]
mod test {
    use crate::card::FsrsMemoryState;
    use crate::prelude::*;

    /// Add a `Basic` note tagged with each of `tags`, returning its id.
    fn add_tagged_note(col: &mut Collection, front: &str, tags: &[&str]) -> NoteId {
        let nt = col.get_notetype_by_name("Basic").unwrap().unwrap();
        let mut note = nt.new_note();
        note.set_field(0, front).unwrap();
        note.tags = tags.iter().map(|t| (*t).to_string()).collect();
        col.add_note(&mut note, DeckId(1)).unwrap();
        note.id
    }

    /// Give the note's first card a fresh FSRS memory state so its concept has
    /// a scored card contributing to Memory.
    fn give_card_memory_state(col: &mut Collection, note_id: NoteId) {
        let cid = col
            .storage
            .all_card_ids_of_note_in_template_order(note_id)
            .unwrap()[0];
        let mut card = col.storage.get_card(cid).unwrap().unwrap();
        card.memory_state = Some(FsrsMemoryState {
            stability: 100.0,
            difficulty: 5.0,
        });
        card.last_review_time = Some(TimestampSecs::now());
        col.storage.update_card(&card).unwrap();
    }

    #[test]
    fn nodes_from_tagged_cards_edges_from_seed() -> Result<()> {
        let mut col = Collection::new();
        // Two concepts joined by a prerequisite edge
        // (amino_acids -> protein_structure), built in-test since the
        // production spine seed is now empty.
        let a = add_tagged_note(&mut col, "a", &["concept::BB::1A::amino_acids"]);
        give_card_memory_state(&mut col, a);
        add_tagged_note(&mut col, "b", &["concept::BB::1A::protein_structure"]);
        crate::storage::concept::edges::add_test_concept_edge(
            &col,
            "concept::BB::1A::amino_acids",
            "concept::BB::1A::protein_structure",
        );

        let resp = col.concept_graph("")?;

        // Both tagged concepts are nodes.
        let node_tags: Vec<&str> = resp.nodes.iter().map(|n| n.concept.as_str()).collect();
        assert!(node_tags.contains(&"concept::BB::1A::amino_acids"));
        assert!(node_tags.contains(&"concept::BB::1A::protein_structure"));

        // The node carries the Memory signal from concept_memory.
        let amino = resp
            .nodes
            .iter()
            .find(|n| n.concept == "concept::BB::1A::amino_acids")
            .unwrap();
        assert_eq!(amino.section, "BB");
        assert_eq!(amino.card_count, 1);
        assert_eq!(amino.scored_card_count, 1);
        assert!(amino.memory > 99.0, "memory was {}", amino.memory);

        // The seed prerequisite edge between them is present and directed.
        let edge = resp
            .edges
            .iter()
            .find(|e| e.from_concept == "concept::BB::1A::amino_acids")
            .expect("seed edge present");
        assert_eq!(edge.to_concept, "concept::BB::1A::protein_structure");
        Ok(())
    }

    #[test]
    fn edges_pruned_to_visible_nodes() -> Result<()> {
        let mut col = Collection::new();
        // Only tag the prerequisite end of a seed edge; the dependent has no
        // card, so it is not a node, and the edge must be dropped.
        add_tagged_note(&mut col, "a", &["concept::BB::1A::amino_acids"]);

        let resp = col.concept_graph("")?;
        assert!(resp
            .nodes
            .iter()
            .any(|n| n.concept == "concept::BB::1A::amino_acids"));
        assert!(!resp
            .nodes
            .iter()
            .any(|n| n.concept == "concept::BB::1A::protein_structure"));
        // No edge may reference the missing dependent.
        assert!(resp
            .edges
            .iter()
            .all(|e| e.to_concept != "concept::BB::1A::protein_structure"
                && e.from_concept != "concept::BB::1A::protein_structure"));
        Ok(())
    }

    #[test]
    fn search_scopes_nodes_and_their_edges() -> Result<()> {
        let mut col = Collection::new();
        // A seed-connected pair, plus an unrelated concept.
        add_tagged_note(&mut col, "a", &["concept::BB::1A::amino_acids"]);
        add_tagged_note(&mut col, "b", &["concept::BB::1A::protein_structure"]);
        add_tagged_note(&mut col, "c", &["concept::CP::4A::translational_motion"]);

        // Scope to a single concept: only that node, and — because the other
        // endpoint is excluded — none of its edges.
        let resp = col.concept_graph("tag:concept::BB::1A::amino_acids")?;
        assert_eq!(resp.nodes.len(), 1);
        assert_eq!(resp.nodes[0].concept, "concept::BB::1A::amino_acids");
        assert!(resp.edges.is_empty());

        // A search matching nothing yields an empty graph, not an error.
        let resp = col.concept_graph("tag:concept::nope::missing")?;
        assert!(resp.nodes.is_empty());
        assert!(resp.edges.is_empty());
        Ok(())
    }
}
