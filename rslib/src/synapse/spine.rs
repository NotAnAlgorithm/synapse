// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Vendored AAMC MCAT content spine — the source of truth for concept identity.
//!
//! `mcat_spine.json` (beside this file) is a projection of the AAMC MCAT content
//! outline plus authored Synapse prerequisites. Each topic carries a canonical
//! concept tag of the form `concept::<section>::<category>::<topic>`, e.g.
//! `concept::BB::1A::amino_acids`, where `<section>` is the AAMC section
//! (BB/CP/PS) and `<category>` the content-category code (1A, 4C, …).
//!
//! The spine is parsed once via [`include_str!`] + `serde_json` and exposed as:
//! - [`all_topics`] — every topic, used to build the coverage expected-set;
//! - [`seed_edges`] — the authored prerequisite graph as `(from, to)` tag pairs,
//!   where `from` is a PREREQUISITE of `to`.

use std::sync::OnceLock;

use serde::Deserialize;

/// A single MCAT content topic from the spine.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Topic {
    /// AAMC section code: `BB`, `CP`, or `PS`.
    pub section: String,
    /// Content-category code, e.g. `1A`.
    pub category: String,
    /// Human-readable category name.
    #[serde(default)]
    pub category_name: String,
    /// Human-readable topic name.
    pub name: String,
    /// Canonical concept tag, e.g. `concept::BB::1A::amino_acids`.
    pub tag: String,
    /// Canonical tags that are prerequisites OF this topic.
    #[serde(default)]
    pub prerequisites: Vec<String>,
}

/// Parsed representation of `mcat_spine.json`.
#[derive(Debug, Clone, Deserialize)]
struct Spine {
    #[serde(default)]
    topics: Vec<Topic>,
}

/// Raw JSON, embedded at compile time so the spine has no runtime dependency.
const SPINE_JSON: &str = include_str!("mcat_spine.json");

/// Lazily-parsed spine, shared across the process.
static SPINE: OnceLock<Spine> = OnceLock::new();

fn spine() -> &'static Spine {
    SPINE.get_or_init(|| {
        serde_json::from_str(SPINE_JSON).expect("vendored mcat_spine.json is valid")
    })
}

/// Every topic in the vendored spine.
pub(crate) fn all_topics() -> &'static [Topic] {
    &spine().topics
}

/// The authored prerequisite graph as `(prerequisite_tag, dependent_tag)` pairs.
///
/// For every topic `T` and every `p` in `T.prerequisites`, yields
/// `(p, T.tag)` — i.e. `from` = prerequisite, `to` = dependent. Ids are resolved
/// at load time, so only the canonical tags need be stable.
pub(crate) fn seed_edges() -> Vec<(String, String)> {
    let mut edges = Vec::new();
    for topic in all_topics() {
        for prereq in &topic.prerequisites {
            edges.push((prereq.clone(), topic.tag.clone()));
        }
    }
    edges
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn spine_parses_with_expected_shape() {
        let topics = all_topics();
        assert_eq!(topics.len(), 161);

        // Three AAMC sections.
        let mut sections: Vec<&str> = topics.iter().map(|t| t.section.as_str()).collect();
        sections.sort_unstable();
        sections.dedup();
        assert_eq!(sections, ["BB", "CP", "PS"]);

        // 31 content categories across the spine.
        let mut cats: Vec<&str> = topics.iter().map(|t| t.category.as_str()).collect();
        cats.sort_unstable();
        cats.dedup();
        assert_eq!(cats.len(), 31);

        // A known canonical tag with its category name populated.
        let amino = topics
            .iter()
            .find(|t| t.tag == "concept::BB::1A::amino_acids")
            .expect("amino_acids topic present");
        assert_eq!(amino.section, "BB");
        assert_eq!(amino.category, "1A");
        assert_eq!(amino.name, "Amino Acids");
        assert!(!amino.category_name.is_empty());
    }

    #[test]
    fn seed_edges_are_prereq_to_dependent_pairs() {
        let edges = seed_edges();
        // 8 authored prerequisite edges across BB/CP/PS.
        assert_eq!(edges.len(), 8);

        // protein_structure depends on amino_acids, so (amino_acids ->
        // protein_structure) is a from=prereq, to=dependent pair.
        assert!(edges.contains(&(
            "concept::BB::1A::amino_acids".to_string(),
            "concept::BB::1A::protein_structure".to_string(),
        )));
    }
}
