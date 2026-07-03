// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//
// Pure computation helpers for the Synapse AAMC coverage checker (PRD B4).
//
// These take the ConceptCoverageResponse returned by the `conceptCoverage`
// backend RPC (categories + section/collection rollups computed against a seed
// AAMC outline) and derive the small view-model the page needs: a flat list of
// "gaps" (expected concepts with no card yet) and section/category labels.
// Kept free of Svelte/DOM so the aggregation logic is easy to test.
//

import type {
    ConceptCoverageResponse_Category as Category,
    ConceptCoverageResponse_ExpectedConcept as ExpectedConcept,
} from "@generated/anki/stats_pb";

/** A single "topic you haven't studied" entry, with its outline location. */
export interface Gap {
    /** full concept tag, e.g. concept::biochem::amino_acid_charge */
    concept: string;
    /** human-readable concept name from the outline */
    name: string;
    /** outline section, e.g. biochem */
    section: string;
    /** human-readable category name the concept belongs to */
    categoryName: string;
}

/**
 * Flatten the outline into the list of gaps (uncovered expected concepts),
 * preserving the outline's category order. This is the "topics you haven't
 * studied" list the page leads its call-to-action with.
 */
export function gaps(categories: readonly Category[]): Gap[] {
    const out: Gap[] = [];
    for (const category of categories) {
        for (const concept of category.concepts) {
            if (!concept.covered) {
                out.push({
                    concept: concept.concept,
                    name: concept.name,
                    section: category.section,
                    categoryName: category.name,
                });
            }
        }
    }
    return out;
}

/**
 * Group categories by their outline section, preserving first-seen order so the
 * page renders sections in the outline's declared order.
 */
export function categoriesBySection(
    categories: readonly Category[],
): { section: string; categories: Category[] }[] {
    const order: string[] = [];
    const groups = new Map<string, Category[]>();
    for (const category of categories) {
        const existing = groups.get(category.section);
        if (existing) {
            existing.push(category);
        } else {
            order.push(category.section);
            groups.set(category.section, [category]);
        }
    }
    return order.map((section) => ({
        section,
        categories: groups.get(section)!,
    }));
}

/** Count of covered / total expected concepts in a category (for the label). */
export function categoryCounts(category: Category): { covered: number; total: number } {
    return { covered: category.coveredCount, total: category.expectedCount };
}

/** Human-friendly section label (underscores → spaces; empty → fallback). */
export function sectionLabel(section: string): string {
    const cleaned = section.replace(/_/g, " ").trim();
    return cleaned.length > 0 ? cleaned : "Other";
}

/** Format a 0..100 coverage value as a rounded percentage string. */
export function coveragePct(coverage: number): string {
    return `${Math.round(coverage)}%`;
}

/** Whether the expected concept is a gap (no card yet). */
export function isGap(concept: ExpectedConcept): boolean {
    return !concept.covered;
}
