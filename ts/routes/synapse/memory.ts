// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//
// Pure computation helpers for the Synapse M0 "Memory" dashboard.
//
// These take the flat list of ConceptScore rows returned by the
// `conceptMemory` backend RPC and derive the learning-first rollups the
// dashboard leads with (overall Memory, per-section rollups, weak spots).
// Kept free of Svelte/DOM so the aggregation logic is easy to test/reason
// about. See PRD features E1 (learning over activity), F1 (rollups) and
// F4 (abstain when data is thin).
//

import type { ConceptMemoryResponse_ConceptScore as ConceptScore } from "@generated/anki/stats_pb";
import { localeCompare } from "@tslib/i18n";

/** Minimum number of concepts with sufficient data before we're willing to
 * show an overall Memory number rather than an F4 "keep practicing" state. */
export const MIN_SUFFICIENT_CONCEPTS = 3;

/** How many weak spots the "Work on next" list surfaces. */
export const WORK_ON_NEXT_LIMIT = 5;

export interface OverallMemory {
    /** Weighted-mean Memory (0..100) across sufficient concepts, or null when
     * we abstain because too few concepts have enough data (F4). */
    memory: number | null;
    /** Concepts (with sufficient data) that fed the number. */
    sufficientConcepts: number;
    /** All concepts returned, regardless of data sufficiency. */
    totalConcepts: number;
    /** Scored cards backing the number — drives the "how sure" hint. */
    scoredCardCount: number;
    /** True when we abstained due to insufficient data (F4 state). */
    abstained: boolean;
}

export interface SectionRollup {
    section: string;
    /** Weighted-mean Memory over the section's sufficient concepts, or null
     * when none of its concepts have enough data yet. */
    memory: number | null;
    /** Concepts in the section that have sufficient data. */
    sufficientConcepts: number;
    /** Total concepts mapped to the section (coverage, secondary). */
    totalConcepts: number;
    /** Total cards mapped to the section (coverage, secondary). */
    cardCount: number;
}

/**
 * Weighted mean of `memory` over the given rows, weighting each concept by
 * its `scoredCardCount` and ignoring concepts without sufficient data.
 *
 * Returns null when no row qualifies (no sufficient data, or zero weight),
 * so callers can render an abstention state instead of a misleading 0.
 */
export function weightedMemory(rows: readonly ConceptScore[]): number | null {
    let weightedSum = 0;
    let weight = 0;
    for (const row of rows) {
        if (!row.sufficientData || row.scoredCardCount <= 0) {
            continue;
        }
        weightedSum += row.memory * row.scoredCardCount;
        weight += row.scoredCardCount;
    }
    if (weight <= 0) {
        return null;
    }
    return weightedSum / weight;
}

/**
 * Overall Memory across all concepts, applying the F4 abstention rule: if
 * fewer than MIN_SUFFICIENT_CONCEPTS concepts have sufficient data we return
 * `memory: null` / `abstained: true` so the UI shows a "keep practicing"
 * state rather than a number built on too little evidence.
 */
export function overallMemory(concepts: readonly ConceptScore[]): OverallMemory {
    const sufficient = concepts.filter((c) => c.sufficientData && c.scoredCardCount > 0);
    const scoredCardCount = sufficient.reduce((sum, c) => sum + c.scoredCardCount, 0);
    const enoughData = sufficient.length >= MIN_SUFFICIENT_CONCEPTS;

    return {
        memory: enoughData ? weightedMemory(sufficient) : null,
        sufficientConcepts: sufficient.length,
        totalConcepts: concepts.length,
        scoredCardCount,
        abstained: !enoughData,
    };
}

/**
 * Per-section rollups (F1). Groups concepts by `section` and computes a
 * weighted-mean Memory over each section's sufficient concepts, alongside
 * coverage (concept/card counts). Sorted by section name for stable display;
 * unsectioned concepts (empty `section`) are grouped under "" and sorted last.
 */
export function sectionRollups(concepts: readonly ConceptScore[]): SectionRollup[] {
    const groups = new Map<string, ConceptScore[]>();
    for (const concept of concepts) {
        const key = concept.section;
        const existing = groups.get(key);
        if (existing) {
            existing.push(concept);
        } else {
            groups.set(key, [concept]);
        }
    }

    const rollups: SectionRollup[] = [];
    for (const [section, rows] of groups) {
        const sufficient = rows.filter((r) => r.sufficientData && r.scoredCardCount > 0);
        rollups.push({
            section,
            memory: weightedMemory(rows),
            sufficientConcepts: sufficient.length,
            totalConcepts: rows.length,
            cardCount: rows.reduce((sum, r) => sum + r.cardCount, 0),
        });
    }

    return rollups.sort((a, b) => {
        // Keep unsectioned ("") concepts at the end.
        if (a.section === "" && b.section !== "") {
            return 1;
        }
        if (b.section === "" && a.section !== "") {
            return -1;
        }
        return localeCompare(a.section, b.section);
    });
}

/**
 * The "Work on next" list (F1): the concepts with the LOWEST Memory among
 * those with sufficient data — the highest-leverage weak spots. Concepts
 * without sufficient data are excluded (we can't say they're weak yet).
 */
export function workOnNext(
    concepts: readonly ConceptScore[],
    limit = WORK_ON_NEXT_LIMIT,
): ConceptScore[] {
    return concepts
        .filter((c) => c.sufficientData && c.scoredCardCount > 0)
        .sort((a, b) => a.memory - b.memory)
        .slice(0, limit);
}

/**
 * Human-friendly concept label: strip the leading `concept::section::`
 * segments and show the leaf tag with underscores turned into spaces.
 * Falls back gracefully for tags that don't follow the convention.
 */
export function conceptLabel(concept: string): string {
    const segments = concept.split("::");
    const leaf = segments[segments.length - 1] ?? concept;
    const cleaned = leaf.replace(/_/g, " ").trim();
    return cleaned.length > 0 ? cleaned : concept;
}

/** Human-friendly section label (underscores → spaces; empty → fallback). */
export function sectionLabel(section: string): string {
    const cleaned = section.replace(/_/g, " ").trim();
    return cleaned.length > 0 ? cleaned : "Uncategorized";
}

/**
 * Plain-language "how sure are we" hint for a Memory figure, keyed off how
 * much scored data backs it. Deliberately qualitative — this dashboard leads
 * with confidence in words, not raw counts (E1).
 */
export function confidenceHint(scoredCardCount: number): string {
    if (scoredCardCount >= 60) {
        return "Based on a lot of your practice — a confident estimate.";
    }
    if (scoredCardCount >= 20) {
        return "Based on a fair amount of practice — a reasonable estimate.";
    }
    return "Based on limited practice — treat this as a rough estimate.";
}

/** Rough plain-language band for a 0..100 Memory value. */
export function memoryBand(memory: number): string {
    if (memory >= 90) {
        return "Strong";
    }
    if (memory >= 75) {
        return "Solid";
    }
    if (memory >= 60) {
        return "Shaky";
    }
    return "Needs work";
}
