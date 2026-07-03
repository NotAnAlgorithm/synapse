// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//
// Pure presentation helpers for the Synapse PROVISIONAL Performance section
// (PRD F2).
//
// The backend read-model (rslib/src/stats/performance.rs) already does the
// aggregation: per-concept application accuracy, current retrievability, and
// prerequisite mastery blended into an *uncalibrated* provisional Performance
// score. These helpers only rank and label those rows.
//
// Two honesty rules from the PRD shape the copy here:
//   - Performance is "can you APPLY this to a novel item" — distinct from, and
//     wider-ranged than, Memory (F1) which is bare recall.
//   - The number is PROVISIONAL / uncalibrated (no held-out AAMC data yet), so
//     everything is labelled preliminary and thin data (F4) is flagged rather
//     than dressed up as confident.
//
// Kept free of Svelte/DOM so the ranking/labelling logic is easy to reason
// about.
//

import type { ConceptPerformanceResponse_ConceptScore as ConceptScore } from "@generated/anki/stats_pb";

/** How many weak spots the "Practice applying" list surfaces. */
export const WEAKEST_APPLICATION_LIMIT = 5;

/**
 * Does the collection have *any* application-form review history? When no
 * concept has an applied review we show an honest empty state rather than a
 * grid of zeros — Performance simply has nothing to say yet.
 */
export function hasApplicationData(concepts: readonly ConceptScore[]): boolean {
    return concepts.some((c) => c.appliedCount > 0);
}

/**
 * Concepts with at least one application review, ranked by the provisional
 * Performance score ascending (weakest first) — the highest-leverage things to
 * practise applying. Concepts with no application history are excluded (we have
 * nothing to rank them on yet).
 */
export function weakestApplications(
    concepts: readonly ConceptScore[],
    limit = WEAKEST_APPLICATION_LIMIT,
): ConceptScore[] {
    return concepts
        .filter((c) => c.appliedCount > 0)
        .sort((a, b) => a.performance - b.performance)
        .slice(0, limit);
}

/**
 * Rough plain-language band for a 0..100 provisional Performance value.
 * Deliberately hedged ("looks", "so far") because the number is uncalibrated.
 */
export function performanceBand(performance: number): string {
    if (performance >= 80) {
        return "Applying well";
    }
    if (performance >= 60) {
        return "Getting there";
    }
    if (performance >= 40) {
        return "Shaky on application";
    }
    return "Needs application practice";
}

/**
 * The main drivers behind a concept's provisional score, as short phrases — the
 * F4 display contract asks every score to show *why*. Surfaces the weakest
 * contributing factor(s) so the learner knows what to fix.
 */
export function performanceDrivers(concept: ConceptScore): string[] {
    const drivers: string[] = [];
    const accuracyPct = Math.round(concept.applicationAccuracy * 100);
    drivers.push(`${accuracyPct}% correct on application items`);
    if (concept.prereqMastery < 0.75) {
        drivers.push("weak prerequisites are holding this back");
    }
    if (concept.retrievability < 0.6) {
        drivers.push("recall of the underlying facts has faded");
    }
    return drivers;
}
