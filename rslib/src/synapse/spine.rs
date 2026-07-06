// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! AAMC MCAT content spine — the single source of truth for concept identity.
//!
//! Reads the tracked spine at `data/mcat/mcat_content_spine.json` (the same file
//! the content team and the card assembler use) at compile time via
//! [`include_str!`]. Each topic is assigned a canonical concept tag of the form
//! `concept::<section>::<category>::<topic>`, e.g. `concept::BB::1A::amino_acids`,
//! where `<section>` is the AAMC section (BB/CP/PS), `<category>` the
//! content-category code (1A, 4C, …), and `<topic>` a slug of the topic id
//! (slugged to match the card assembler, so generated cards and spine-derived
//! nodes share the exact same tag).
//!
//! Exposed as:
//! - [`all_topics`] — every topic, used to build the coverage expected-set;
//! - [`seed_edges`] — the authored prerequisite graph as `(from, to)` tag pairs,
//!   where `from` is a PREREQUISITE of `to`. Topic `prerequisites` in the JSON
//!   (if any) are authored as topic ids and resolved to canonical tags here.
//!   The authored prerequisite edges have been removed pending research, so this
//!   currently yields no edges; the machinery remains for when they return.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

// --- Raw (nested) shape of data/mcat/mcat_content_spine.json ----------------
// Only the fields we need are declared; everything else (meta, disciplines,
// subtopics, weights, …) is ignored by serde.

#[derive(Deserialize)]
struct RawSpine {
    #[serde(default)]
    sections: Vec<RawSection>,
}

#[derive(Deserialize)]
struct RawSection {
    id: String,
    #[serde(default)]
    foundational_concepts: Vec<RawFc>,
}

#[derive(Deserialize)]
struct RawFc {
    #[serde(default)]
    content_categories: Vec<RawCategory>,
}

#[derive(Deserialize)]
struct RawCategory {
    code: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    topics: Vec<RawTopic>,
}

#[derive(Deserialize)]
struct RawTopic {
    id: String,
    name: String,
    /// Prerequisite topic ids (e.g. `1A.amino-acids`); resolved to tags below.
    #[serde(default)]
    prerequisites: Vec<String>,
}

/// A single MCAT content topic, resolved from the nested spine.
#[derive(Debug, Clone)]
pub(crate) struct Topic {
    /// AAMC section code: `BB`, `CP`, or `PS`.
    pub section: String,
    /// Content-category code, e.g. `1A`.
    pub category: String,
    /// Human-readable category name.
    pub category_name: String,
    /// Human-readable topic name.
    pub name: String,
    /// Canonical concept tag, e.g. `concept::BB::1A::amino_acids`.
    pub tag: String,
    /// Canonical tags that are prerequisites OF this topic.
    pub prerequisites: Vec<String>,
}

/// Raw spine JSON, embedded at compile time so the spine has no runtime file
/// dependency. Path is relative to this source file (`rslib/src/synapse/`).
const SPINE_JSON: &str = include_str!("../../../data/mcat/mcat_content_spine.json");

/// Lazily-built topic list, shared across the process.
static TOPICS: OnceLock<Vec<Topic>> = OnceLock::new();

/// Slug of a topic id's leaf (the part after the first `.`): lowercased, with
/// runs of non-alphanumeric characters collapsed to a single `_` and trimmed.
/// Matches the card assembler's slugging exactly.
fn topic_slug(topic_id: &str) -> String {
    let leaf = topic_id.split_once('.').map(|(_, r)| r).unwrap_or(topic_id);
    let mut out = String::new();
    let mut pending_sep = false;
    for ch in leaf.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('_');
            }
            pending_sep = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_sep = true;
        }
    }
    out
}

/// The canonical concept tag for a topic: `concept::<section>::<category>::<slug>`.
fn topic_tag(section: &str, category: &str, topic_id: &str) -> String {
    format!("concept::{}::{}::{}", section, category, topic_slug(topic_id))
}

fn build() -> Vec<Topic> {
    let raw: RawSpine = serde_json::from_str(SPINE_JSON)
        .expect("data/mcat/mcat_content_spine.json is valid JSON");

    struct Pending {
        section: String,
        category: String,
        category_name: String,
        name: String,
        tag: String,
        prereq_ids: Vec<String>,
    }

    // First pass: assign canonical tags and index topic id -> tag so
    // prerequisites (authored as topic ids) can be resolved.
    let mut pending = Vec::new();
    let mut id_to_tag: HashMap<String, String> = HashMap::new();
    for section in &raw.sections {
        for fc in &section.foundational_concepts {
            for cat in &fc.content_categories {
                for topic in &cat.topics {
                    let tag = topic_tag(&section.id, &cat.code, &topic.id);
                    id_to_tag.insert(topic.id.clone(), tag.clone());
                    pending.push(Pending {
                        section: section.id.clone(),
                        category: cat.code.clone(),
                        category_name: cat.title.clone(),
                        name: topic.name.clone(),
                        tag,
                        prereq_ids: topic.prerequisites.clone(),
                    });
                }
            }
        }
    }

    // Second pass: resolve prerequisite topic ids to canonical tags. An id that
    // doesn't resolve (typo in the spine) is dropped rather than panicking.
    pending
        .into_iter()
        .map(|p| Topic {
            section: p.section,
            category: p.category,
            category_name: p.category_name,
            name: p.name,
            tag: p.tag,
            prerequisites: p
                .prereq_ids
                .iter()
                .filter_map(|id| id_to_tag.get(id).cloned())
                .collect(),
        })
        .collect()
}

/// Every topic in the spine.
pub(crate) fn all_topics() -> &'static [Topic] {
    TOPICS.get_or_init(build)
}

/// The authored prerequisite graph as `(prerequisite_tag, dependent_tag)` pairs.
///
/// For every topic `T` and every `p` in `T.prerequisites`, yields `(p, T.tag)` —
/// i.e. `from` = prerequisite, `to` = dependent.
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
        // Authored prerequisite edges have been removed pending research, so the
        // spine currently declares no prerequisites and seed_edges() is empty.
        // The resolution machinery is retained for when the edges return.
        assert!(seed_edges().is_empty());
    }
}
