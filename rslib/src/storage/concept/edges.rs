// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Synapse prerequisite knowledge graph over concepts (schema 20).
//!
//! The `concept_edges` table is a LOCAL, DERIVED structure — like the
//! `concepts`/`card_concepts` projection it references, it is never synced and
//! is dropped by the schema-18 downgrade, then reconstructed on next open.
//!
//! **Edge direction.** A row `(from_concept_id, to_concept_id)` means "`from`
//! is a PREREQUISITE of `to`" — you should master `from` before `to`.
//! Equivalently, `to` DEPENDS ON `from`. All accessors below are documented in
//! these terms:
//! - [`SqliteStorage::get_prerequisites`] of X = the concepts X depends on
//!   (rows where X is `to`).
//! - [`SqliteStorage::get_dependents`] of X = the concepts that depend on X
//!   (rows where X is `from`).
//!
//! Because concept ids are local/derived (assigned append-only from
//! `concept::` note tags), the graph is authored as a static SEED of
//! `(from_tag, to_tag)` pairs ([`SEED_EDGES`]) and resolved to ids via
//! [`SqliteStorage::get_or_create_concept`] by
//! [`SqliteStorage::rebuild_concept_edges_from_seed`], which the schema-20
//! migration runs and which doubles as a repair entry point.

use rusqlite::params;

use super::ConceptId;
use super::SqliteStorage;
use crate::error::Result;
use crate::prelude::*;

/// A prerequisite edge in the concept graph: `from` is a prerequisite of `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConceptEdge {
    pub from: ConceptId,
    pub to: ConceptId,
}

/// Authored prerequisite seed, expressed as `(prerequisite_tag, dependent_tag)`
/// over the M1 demo concepts. Each pair reads "master the first before the
/// second". Extend this table as the concept catalogue grows; ids are resolved
/// at load time so only the tags need be stable.
///
/// The referenced concept tags are created on demand when the seed is loaded,
/// so a pair may be authored before any note carries the tag.
pub(crate) const SEED_EDGES: &[(&str, &str)] = &[
    // --- Biochemistry ---
    // Understanding amino-acid charge underpins reasoning about enzyme
    // structure/kinetics and buffering.
    (
        "concept::biochem::amino_acid_charge",
        "concept::biochem::enzyme_kinetics",
    ),
    (
        "concept::biochem::amino_acid_charge",
        "concept::biochem::protein_structure",
    ),
    (
        "concept::biochem::protein_structure",
        "concept::biochem::enzyme_kinetics",
    ),
    // --- Physics ---
    // Kinematics precedes dynamics; both feed energy; energy precedes optics'
    // wave/energy treatment.
    (
        "concept::physics::kinematics",
        "concept::physics::newtons_laws",
    ),
    ("concept::physics::newtons_laws", "concept::physics::energy"),
    ("concept::physics::energy", "concept::physics::optics"),
    // --- Psychology / behavioural sciences ---
    // Neurons underlie sensation, which underlies perception.
    ("concept::psych::neurons", "concept::psych::sensation"),
    ("concept::psych::sensation", "concept::psych::perception"),
];

impl SqliteStorage {
    /// Insert a prerequisite edge (`from` is a prerequisite of `to`).
    /// Idempotent — a repeated edge is ignored rather than erroring. A
    /// self-edge is rejected as it would make a concept its own
    /// prerequisite.
    pub(crate) fn add_concept_edge(&self, from: ConceptId, to: ConceptId) -> Result<()> {
        require!(from != to, "concept cannot be its own prerequisite");
        self.db
            .prepare_cached(
                "INSERT OR IGNORE INTO concept_edges (from_concept_id, to_concept_id, mtime_secs) \
                 VALUES (?, ?, ?)",
            )?
            .execute(params![from, to, TimestampSecs::now()])?;
        Ok(())
    }

    /// The prerequisites of `concept` — the concepts it depends on (rows where
    /// `concept` is the dependent `to`). Ordered by id for determinism.
    pub(crate) fn get_prerequisites(&self, concept: ConceptId) -> Result<Vec<ConceptId>> {
        self.db
            .prepare_cached(
                "SELECT from_concept_id FROM concept_edges WHERE to_concept_id = ? \
                 ORDER BY from_concept_id",
            )?
            .query_and_then([concept], |r| Ok(ConceptId(r.get(0)?)))?
            .collect()
    }

    /// The dependents of `concept` — the concepts that require it (rows where
    /// `concept` is the prerequisite `from`). Ordered by id for determinism.
    #[allow(dead_code)] // symmetric graph API (dependents); for M2+ graph views
    pub(crate) fn get_dependents(&self, concept: ConceptId) -> Result<Vec<ConceptId>> {
        self.db
            .prepare_cached(
                "SELECT to_concept_id FROM concept_edges WHERE from_concept_id = ? \
                 ORDER BY to_concept_id",
            )?
            .query_and_then([concept], |r| Ok(ConceptId(r.get(0)?)))?
            .collect()
    }

    /// All edges in the graph, ordered by (from, to). Used by tests and repair
    /// tooling.
    #[allow(dead_code)]
    #[allow(dead_code)] // full edge enumeration; for M2+ graph views / export
    pub(crate) fn all_concept_edges(&self) -> Result<Vec<ConceptEdge>> {
        self.db
            .prepare_cached(
                "SELECT from_concept_id, to_concept_id FROM concept_edges \
                 ORDER BY from_concept_id, to_concept_id",
            )?
            .query_and_then([], |r| {
                Ok(ConceptEdge {
                    from: ConceptId(r.get(0)?),
                    to: ConceptId(r.get(1)?),
                })
            })?
            .collect()
    }

    /// (Re)load the authored [`SEED_EDGES`] into `concept_edges`, resolving
    /// each tag pair to (local, derived) concept ids. Existing edges are
    /// left in place (insert-or-ignore), so this is safe to run repeatedly
    /// and is used both by the schema-20 migration and as a repair entry
    /// point.
    ///
    /// The referenced concepts are created if missing, mirroring how the
    /// concept projection assigns stable append-only ids from tags.
    pub(crate) fn rebuild_concept_edges_from_seed(&self) -> Result<()> {
        for (from_tag, to_tag) in SEED_EDGES {
            // Defensive: an authored pair with identical tags would resolve to
            // the same id and be rejected by `add_concept_edge` as a self-edge,
            // which — running inside the schema-20 migration — would prevent the
            // collection from opening. Skip such pairs rather than error.
            if from_tag == to_tag {
                continue;
            }
            let from = self.get_or_create_concept(from_tag)?;
            let to = self.get_or_create_concept(to_tag)?;
            self.add_concept_edge(from, to)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn seed_edges_load_and_are_queryable() -> Result<()> {
        let col = Collection::new();
        // The schema-20 migration ran the seed load on open.
        let edges = col.storage.all_concept_edges()?;
        assert_eq!(edges.len(), SEED_EDGES.len());

        // amino_acid_charge is a prerequisite of enzyme_kinetics.
        let amino = col
            .storage
            .get_concept_id_by_tag("concept::biochem::amino_acid_charge")?
            .unwrap();
        let kinetics = col
            .storage
            .get_concept_id_by_tag("concept::biochem::enzyme_kinetics")?
            .unwrap();
        let structure = col
            .storage
            .get_concept_id_by_tag("concept::biochem::protein_structure")?
            .unwrap();

        // enzyme_kinetics depends on both amino_acid_charge and protein_structure.
        let prereqs = col.storage.get_prerequisites(kinetics)?;
        assert!(prereqs.contains(&amino));
        assert!(prereqs.contains(&structure));

        // amino_acid_charge is a prerequisite of both enzyme_kinetics and
        // protein_structure (its dependents), but has no prerequisites of its
        // own.
        assert!(col.storage.get_prerequisites(amino)?.is_empty());
        let dependents = col.storage.get_dependents(amino)?;
        assert!(dependents.contains(&kinetics));
        assert!(dependents.contains(&structure));

        Ok(())
    }

    #[test]
    fn rebuild_is_idempotent() -> Result<()> {
        let col = Collection::new();
        let before = col.storage.all_concept_edges()?;
        col.storage.rebuild_concept_edges_from_seed()?;
        let after = col.storage.all_concept_edges()?;
        assert_eq!(before, after);
        Ok(())
    }

    #[test]
    fn add_edge_dedupes_and_rejects_self_edge() -> Result<()> {
        let col = Collection::new();
        let a = col.storage.get_or_create_concept("concept::test::a")?;
        let b = col.storage.get_or_create_concept("concept::test::b")?;

        col.storage.add_concept_edge(a, b)?;
        let count_after_first = col
            .storage
            .get_dependents(a)?
            .into_iter()
            .filter(|d| *d == b)
            .count();
        assert_eq!(count_after_first, 1);
        // repeat is ignored, not duplicated
        col.storage.add_concept_edge(a, b)?;
        assert_eq!(
            col.storage
                .get_dependents(a)?
                .iter()
                .filter(|d| **d == b)
                .count(),
            1
        );

        // self-edge rejected
        assert!(col.storage.add_concept_edge(a, a).is_err());
        Ok(())
    }
}
