// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//
// Pure presentation helpers for the Synapse adoption section (PRD E2/E3).
//
// The backend read-model (rslib/src/stats/adoption.rs) already does the real
// work: difficulty-weighted points and a forgiveness-aware streak. These
// helpers only turn its numbers into the plain-language, encouragement-first
// copy the dashboard shows — deliberately leading with *what the effort meant*
// rather than raw counts (E1 spirit: reward honest, hard work, not padding).
//
// Kept free of Svelte/DOM so the phrasing logic is easy to reason about.
//

import type { AdoptionStatsResponse_HardWin as HardWin } from "@generated/anki/stats_pb";

/**
 * Plain-language streak line. A streak of 0 nudges toward starting; longer
 * streaks celebrate momentum without turning the number into the point of the
 * app. Freeze credits are surfaced as reassurance that one off day is safe.
 */
export function streakSummary(days: number, freezes: number, studiedToday: boolean): string {
    if (days <= 0) {
        return "Study today to start a streak.";
    }
    const dayWord = days === 1 ? "day" : "days";
    const base = studiedToday
        ? `${days}-${dayWord} streak — nice work today.`
        : `${days}-${dayWord} streak — a little study today keeps it going.`;
    if (freezes > 0) {
        const freezeWord = freezes === 1 ? "freeze" : "freezes";
        return `${base} You have ${freezes} ${freezeWord} in reserve, so one off day won't reset it.`;
    }
    return base;
}

/**
 * Plain-language framing for the points total, emphasising that points come
 * from *hard* wins and recoveries rather than easy reps (anti-padding message).
 */
export function pointsSummary(
    points: number,
    successfulReviews: number,
    lapseRecoveries: number,
): string {
    if (successfulReviews <= 0) {
        return "Points come from recalling cards you were close to forgetting — the hard wins.";
    }
    if (lapseRecoveries > 0) {
        const recoveryWord = lapseRecoveries === 1 ? "card" : "cards";
        return `Earned mostly on your hardest recalls, including ${lapseRecoveries} recovered ${recoveryWord} you'd lapsed on.`;
    }
    return "Earned by recalling cards you were close to forgetting — easy reps count for little.";
}

/**
 * How a single hard win reads on the dashboard: a short description keyed off
 * how low the reconstructed retrievability was, plus a recovery note.
 */
export function hardWinLabel(win: HardWin): string {
    if (win.lapseRecovery) {
        return "Recovered a lapsed card";
    }
    const chance = Math.round(win.retrievability * 100);
    if (chance <= 40) {
        return `Recalled at only a ${chance}% chance`;
    }
    if (chance <= 70) {
        return `Recalled against the odds (${chance}%)`;
    }
    return `A solid recall (${chance}%)`;
}
