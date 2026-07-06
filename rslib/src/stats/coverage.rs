// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! AAMC coverage checker (PRD B4).
//!
//! Compares the user's cards against the vendored AAMC MCAT content spine and
//! reports, per content category/section, how many of the expected concepts the
//! user has at least one card for versus the total expected, plus a "gaps" list
//! of expected concepts with no card yet.
//!
//! The expected set is the spine itself: every topic is an expected concept,
//! grouped by its content category (1A, 4C, …) within its AAMC section
//! (BB/CP/PS). Concepts the demo/user has seeded cards for show as covered; the
//! rest are gaps a fresh collection has not yet made cards for.
//!
//! Concept membership (which concepts the user has cards for) is read from the
//! canonical `concept::<section>::<category>::<topic>` note tags via
//! [`Collection::concept_memory`], so this module does NOT depend on any
//! derived concept tables.

use std::collections::HashMap;

use anki_proto::stats::concept_coverage_response::Category;
use anki_proto::stats::concept_coverage_response::ExpectedConcept;
use anki_proto::stats::concept_coverage_response::Section;
use anki_proto::stats::ConceptCoverageResponse;

use crate::prelude::*;
use crate::synapse::spine;

/// Compute `count / total * 100` as a percentage, guarding against a zero
/// denominator (returns 0.0 for an empty category/section).
fn coverage_pct(count: u32, total: u32) -> f32 {
    if total == 0 {
        0.0
    } else {
        count as f32 * 100.0 / total as f32
    }
}

impl Collection {
    /// Coverage of the user's cards (within `search`) against the AAMC MCAT
    /// spine. Read-model behind the Synapse coverage checker (PRD B4).
    ///
    /// `search` scopes which cards count toward coverage (e.g. `deck:MCAT`);
    /// empty means the whole collection.
    pub(crate) fn concept_coverage(&mut self, search: &str) -> Result<ConceptCoverageResponse> {
        // Reuse the concept-memory read-model to learn which concept tags the
        // user has cards for, and how many. `card_count` is the number of cards
        // mapped to each concept tag; a tag with >= 1 card is "covered".
        let memory = self.concept_memory(search)?;
        let card_counts: HashMap<&str, u32> = memory
            .concepts
            .iter()
            .map(|c| (c.concept.as_str(), c.card_count))
            .collect();

        // Group the spine's topics into ordered categories, preserving the
        // spine's topic order and the first-seen order of the categories.
        // category code -> index into `categories`
        let mut cat_index: HashMap<&str, usize> = HashMap::new();
        let mut categories: Vec<Category> = Vec::new();

        for topic in spine::all_topics() {
            let card_count = card_counts.get(topic.tag.as_str()).copied().unwrap_or(0);
            let covered = card_count > 0;
            let expected = ExpectedConcept {
                concept: topic.tag.clone(),
                name: topic.name.clone(),
                covered,
                card_count,
            };

            match cat_index.get(topic.category.as_str()) {
                Some(&idx) => categories[idx].concepts.push(expected),
                None => {
                    cat_index.insert(topic.category.as_str(), categories.len());
                    categories.push(Category {
                        section: topic.section.clone(),
                        id: topic.category.clone(),
                        name: topic.category_name.clone(),
                        concepts: vec![expected],
                        covered_count: 0,
                        expected_count: 0,
                        coverage: 0.0,
                    });
                }
            }
        }

        // section -> (covered, expected)
        let mut section_totals: Vec<(String, u32, u32)> = Vec::new();
        let mut total_covered = 0u32;
        let mut total_expected = 0u32;

        for cat in &mut categories {
            let covered_count = cat.concepts.iter().filter(|c| c.covered).count() as u32;
            let expected_count = cat.concepts.len() as u32;
            cat.covered_count = covered_count;
            cat.expected_count = expected_count;
            cat.coverage = coverage_pct(covered_count, expected_count);

            total_covered += covered_count;
            total_expected += expected_count;

            // Fold into the section rollup.
            match section_totals
                .iter_mut()
                .find(|(s, _, _)| *s == cat.section)
            {
                Some(entry) => {
                    entry.1 += covered_count;
                    entry.2 += expected_count;
                }
                None => section_totals.push((cat.section.clone(), covered_count, expected_count)),
            }
        }

        let sections = section_totals
            .into_iter()
            .map(|(section, covered, expected)| Section {
                section,
                covered_count: covered,
                expected_count: expected,
                coverage: coverage_pct(covered, expected),
            })
            .collect();

        Ok(ConceptCoverageResponse {
            categories,
            sections,
            covered_count: total_covered,
            expected_count: total_expected,
            coverage: coverage_pct(total_covered, total_expected),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Add a `Basic` note tagged with `tag` so its single card is mapped to
    /// that concept, mirroring the Synapse tag convention.
    fn add_tagged_card(col: &mut Collection, tag: &str) {
        let nt = col.get_notetype_by_name("Basic").unwrap().unwrap();
        let mut note = nt.new_note();
        note.tags = vec![tag.to_string()];
        col.add_note(&mut note, DeckId(1)).unwrap();
    }

    #[test]
    fn empty_collection_is_all_gaps() -> Result<()> {
        let mut col = Collection::new();
        let resp = col.concept_coverage("")?;

        // Every spine concept is a gap; nothing is covered.
        assert_eq!(resp.covered_count, 0);
        assert!(resp.expected_count > 0);
        assert_eq!(resp.coverage, 0.0);
        for cat in &resp.categories {
            assert_eq!(cat.covered_count, 0);
            assert!(cat.concepts.iter().all(|c| !c.covered && c.card_count == 0));
        }
        for section in &resp.sections {
            assert_eq!(section.covered_count, 0);
        }
        Ok(())
    }

    #[test]
    fn expected_set_is_the_full_spine() -> Result<()> {
        let mut col = Collection::new();
        let resp = col.concept_coverage("")?;

        // The whole spine is the expected set: 161 topics across 3 AAMC
        // sections and 31 categories.
        assert_eq!(resp.expected_count, 161);
        assert_eq!(resp.categories.len(), 31);

        let mut sections: Vec<&str> = resp.sections.iter().map(|s| s.section.as_str()).collect();
        sections.sort_unstable();
        assert_eq!(sections, ["BB", "CP", "PS"]);

        // The per-category expected counts sum to the collection total.
        let cat_expected: u32 = resp.categories.iter().map(|c| c.expected_count).sum();
        assert_eq!(cat_expected, resp.expected_count);
        Ok(())
    }

    #[test]
    fn covered_and_gap_concepts() -> Result<()> {
        let mut col = Collection::new();
        // Cover one concept in the 1A category, plus one that is not in the
        // spine at all (must be ignored).
        add_tagged_card(&mut col, "concept::BB::1A::amino_acids");
        add_tagged_card(&mut col, "concept::BB::1A::not_in_spine");

        let resp = col.concept_coverage("")?;

        let cat = resp
            .categories
            .iter()
            .find(|c| c.id == "1A")
            .expect("1A category present");
        assert_eq!(cat.section, "BB");
        assert_eq!(cat.covered_count, 1);

        let amino = cat
            .concepts
            .iter()
            .find(|c| c.concept == "concept::BB::1A::amino_acids")
            .unwrap();
        assert!(amino.covered);
        assert_eq!(amino.card_count, 1);

        // A concept with no card is a gap.
        let structure = cat
            .concepts
            .iter()
            .find(|c| c.concept == "concept::BB::1A::protein_structure")
            .unwrap();
        assert!(!structure.covered);
        assert_eq!(structure.card_count, 0);

        // The out-of-spine tag did not inflate any total.
        assert_eq!(resp.covered_count, 1);

        // BB section rollup reflects exactly the one covered concept.
        let bb = resp.sections.iter().find(|s| s.section == "BB").unwrap();
        assert_eq!(bb.covered_count, 1);
        Ok(())
    }

    #[test]
    fn card_count_aggregates_multiple_cards() -> Result<()> {
        let mut col = Collection::new();
        add_tagged_card(&mut col, "concept::CP::4C::circuit_elements");
        add_tagged_card(&mut col, "concept::CP::4C::circuit_elements");

        let resp = col.concept_coverage("")?;
        let circuits = resp.categories.iter().find(|c| c.id == "4C").unwrap();
        assert_eq!(circuits.section, "CP");
        let elements = circuits
            .concepts
            .iter()
            .find(|c| c.concept == "concept::CP::4C::circuit_elements")
            .unwrap();
        assert!(elements.covered);
        assert_eq!(elements.card_count, 2);
        // Two cards on one concept still count as one covered concept.
        assert_eq!(circuits.covered_count, 1);
        Ok(())
    }

    #[test]
    fn search_scopes_coverage() -> Result<()> {
        let mut col = Collection::new();
        add_tagged_card(&mut col, "concept::PS::7C::associative_learning");

        // A search that matches nothing yields all gaps.
        let resp = col.concept_coverage("tag:does_not_exist")?;
        assert_eq!(resp.covered_count, 0);
        Ok(())
    }
}
