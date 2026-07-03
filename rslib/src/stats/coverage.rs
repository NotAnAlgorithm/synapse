// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! AAMC coverage checker (PRD B4).
//!
//! Compares the user's cards against a small hand-made AAMC-style content
//! outline and reports, per outline category/section, how many of the expected
//! concepts the user has at least one card for versus the total expected, plus
//! a "gaps" list of expected concepts with no card yet.
//!
//! The outline is a seed (decision #2): a deliberately small, hand-curated set
//! of biochem/physics/psych categories, each mapping to the concept tags we
//! expect a well-covered deck to contain. It is embedded here as a `const` data
//! structure so the checker has zero external dependencies.
//!
//! Concept membership (which concepts the user has cards for) is read from the
//! `concept::<section>::<id>` note tags via [`Collection::concept_memory`], so
//! this module does NOT depend on any derived concept tables.

use std::collections::HashMap;

use anki_proto::stats::concept_coverage_response::Category;
use anki_proto::stats::concept_coverage_response::ExpectedConcept;
use anki_proto::stats::concept_coverage_response::Section;
use anki_proto::stats::ConceptCoverageResponse;

use crate::prelude::*;

/// One expected concept in the seed outline: its stable concept tag and a
/// human-readable name.
struct OutlineConcept {
    /// full concept tag, e.g. `concept::biochem::amino_acid_charge`
    tag: &'static str,
    /// human-readable concept name
    name: &'static str,
}

/// One outline category, grouping the expected concepts for a slice of an AAMC
/// content section.
struct OutlineCategory {
    /// AAMC section, e.g. `biochem`. Matches the 2nd segment of the concept
    /// tags below.
    section: &'static str,
    /// stable category id, e.g. `biochem::amino_acids`.
    id: &'static str,
    /// human-readable category name.
    name: &'static str,
    /// expected concepts under this category.
    concepts: &'static [OutlineConcept],
}

/// Seed AAMC-style content outline (biochem / physics / psych).
///
/// Hand-made and intentionally small. Some concepts here intersect the demo
/// seed notes provisioned by Synapse (so those show as covered); the rest are
/// gaps a fresh collection has not yet made cards for. The section on each
/// concept tag must match the containing category's `section`.
const OUTLINE: &[OutlineCategory] = &[
    // --- Biochemistry ------------------------------------------------------
    OutlineCategory {
        section: "biochem",
        id: "biochem::amino_acids",
        name: "Amino acids and proteins",
        concepts: &[
            OutlineConcept {
                tag: "concept::biochem::amino_acid_charge",
                name: "Amino acid charge",
            },
            OutlineConcept {
                tag: "concept::biochem::peptide_bond",
                name: "Peptide bond formation",
            },
            OutlineConcept {
                tag: "concept::biochem::protein_structure",
                name: "Protein structure levels",
            },
        ],
    },
    OutlineCategory {
        section: "biochem",
        id: "biochem::enzymes",
        name: "Enzymes",
        concepts: &[
            OutlineConcept {
                tag: "concept::biochem::enzyme_kinetics",
                name: "Enzyme kinetics",
            },
            OutlineConcept {
                tag: "concept::biochem::enzyme_regulation",
                name: "Enzyme regulation",
            },
        ],
    },
    OutlineCategory {
        section: "biochem",
        id: "biochem::metabolism",
        name: "Metabolism",
        concepts: &[
            OutlineConcept {
                tag: "concept::biochem::glycolysis",
                name: "Glycolysis",
            },
            OutlineConcept {
                tag: "concept::biochem::citric_acid_cycle",
                name: "Citric acid cycle",
            },
            OutlineConcept {
                tag: "concept::biochem::oxidative_phosphorylation",
                name: "Oxidative phosphorylation",
            },
        ],
    },
    // --- Physics -----------------------------------------------------------
    OutlineCategory {
        section: "physics",
        id: "physics::circuits",
        name: "Electric circuits",
        concepts: &[
            OutlineConcept {
                tag: "concept::physics::circuits_ohms_law",
                name: "Ohm's law",
            },
            OutlineConcept {
                tag: "concept::physics::circuits_power",
                name: "Electrical power",
            },
        ],
    },
    OutlineCategory {
        section: "physics",
        id: "physics::mechanics",
        name: "Mechanics",
        concepts: &[
            OutlineConcept {
                tag: "concept::physics::kinematics",
                name: "Kinematics",
            },
            OutlineConcept {
                tag: "concept::physics::newtons_laws",
                name: "Newton's laws",
            },
            OutlineConcept {
                tag: "concept::physics::work_energy",
                name: "Work and energy",
            },
        ],
    },
    OutlineCategory {
        section: "physics",
        id: "physics::fluids",
        name: "Fluids",
        concepts: &[
            OutlineConcept {
                tag: "concept::physics::fluid_pressure",
                name: "Fluid pressure",
            },
            OutlineConcept {
                tag: "concept::physics::bernoulli",
                name: "Bernoulli's principle",
            },
        ],
    },
    // --- Psychology / Sociology -------------------------------------------
    OutlineCategory {
        section: "psych",
        id: "psych::learning",
        name: "Learning and conditioning",
        concepts: &[
            OutlineConcept {
                tag: "concept::psych::operant_conditioning",
                name: "Operant conditioning",
            },
            OutlineConcept {
                tag: "concept::psych::classical_conditioning",
                name: "Classical conditioning",
            },
        ],
    },
    OutlineCategory {
        section: "psych",
        id: "psych::memory",
        name: "Memory",
        concepts: &[
            OutlineConcept {
                tag: "concept::psych::encoding",
                name: "Encoding",
            },
            OutlineConcept {
                tag: "concept::psych::retrieval",
                name: "Retrieval",
            },
            OutlineConcept {
                tag: "concept::psych::forgetting",
                name: "Forgetting",
            },
        ],
    },
    OutlineCategory {
        section: "psych",
        id: "psych::social",
        name: "Social psychology",
        concepts: &[
            OutlineConcept {
                tag: "concept::psych::attribution",
                name: "Attribution theory",
            },
            OutlineConcept {
                tag: "concept::psych::conformity",
                name: "Conformity",
            },
        ],
    },
];

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
    /// Coverage of the user's cards (within `search`) against the seed AAMC
    /// outline. Read-model behind the Synapse coverage checker (PRD B4).
    ///
    /// `search` scopes which cards count toward coverage (e.g. `deck:Synapse`);
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

        let mut categories: Vec<Category> = Vec::with_capacity(OUTLINE.len());
        // section -> (covered, expected)
        let mut section_totals: Vec<(&'static str, u32, u32)> = Vec::new();
        let mut total_covered = 0u32;
        let mut total_expected = 0u32;

        for cat in OUTLINE {
            let mut concepts: Vec<ExpectedConcept> = Vec::with_capacity(cat.concepts.len());
            let mut covered_count = 0u32;

            for oc in cat.concepts {
                let card_count = card_counts.get(oc.tag).copied().unwrap_or(0);
                let covered = card_count > 0;
                if covered {
                    covered_count += 1;
                }
                concepts.push(ExpectedConcept {
                    concept: oc.tag.to_string(),
                    name: oc.name.to_string(),
                    covered,
                    card_count,
                });
            }

            let expected_count = cat.concepts.len() as u32;
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
                None => section_totals.push((cat.section, covered_count, expected_count)),
            }

            categories.push(Category {
                section: cat.section.to_string(),
                id: cat.id.to_string(),
                name: cat.name.to_string(),
                concepts,
                covered_count,
                expected_count,
                coverage: coverage_pct(covered_count, expected_count),
            });
        }

        let sections = section_totals
            .into_iter()
            .map(|(section, covered, expected)| Section {
                section: section.to_string(),
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

        // Every outline concept is a gap; nothing is covered.
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
    fn covered_and_gap_concepts() -> Result<()> {
        let mut col = Collection::new();
        // Cover one concept in the amino-acids category, plus one that is not in
        // the outline at all (must be ignored).
        add_tagged_card(&mut col, "concept::biochem::amino_acid_charge");
        add_tagged_card(&mut col, "concept::biochem::not_in_outline");

        let resp = col.concept_coverage("")?;

        let amino = resp
            .categories
            .iter()
            .find(|c| c.id == "biochem::amino_acids")
            .expect("amino-acids category present");
        assert_eq!(amino.covered_count, 1);
        assert_eq!(amino.expected_count, 3);

        let charge = amino
            .concepts
            .iter()
            .find(|c| c.concept == "concept::biochem::amino_acid_charge")
            .unwrap();
        assert!(charge.covered);
        assert_eq!(charge.card_count, 1);

        // A concept with no card is a gap.
        let peptide = amino
            .concepts
            .iter()
            .find(|c| c.concept == "concept::biochem::peptide_bond")
            .unwrap();
        assert!(!peptide.covered);
        assert_eq!(peptide.card_count, 0);

        // The out-of-outline tag did not inflate any total.
        assert_eq!(resp.covered_count, 1);

        // Biochem section rollup reflects exactly the one covered concept.
        let biochem = resp
            .sections
            .iter()
            .find(|s| s.section == "biochem")
            .unwrap();
        assert_eq!(biochem.covered_count, 1);
        Ok(())
    }

    #[test]
    fn card_count_aggregates_multiple_cards() -> Result<()> {
        let mut col = Collection::new();
        add_tagged_card(&mut col, "concept::physics::circuits_ohms_law");
        add_tagged_card(&mut col, "concept::physics::circuits_ohms_law");

        let resp = col.concept_coverage("")?;
        let circuits = resp
            .categories
            .iter()
            .find(|c| c.id == "physics::circuits")
            .unwrap();
        let ohms = circuits
            .concepts
            .iter()
            .find(|c| c.concept == "concept::physics::circuits_ohms_law")
            .unwrap();
        assert!(ohms.covered);
        assert_eq!(ohms.card_count, 2);
        // Two cards on one concept still count as one covered concept.
        assert_eq!(circuits.covered_count, 1);
        Ok(())
    }

    #[test]
    fn search_scopes_coverage() -> Result<()> {
        let mut col = Collection::new();
        add_tagged_card(&mut col, "concept::psych::operant_conditioning");

        // A search that matches nothing yields all gaps.
        let resp = col.concept_coverage("tag:does_not_exist")?;
        assert_eq!(resp.covered_count, 0);
        Ok(())
    }
}
