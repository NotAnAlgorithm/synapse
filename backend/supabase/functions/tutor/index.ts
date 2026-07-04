// Edge Function: tutor — the C2 state-grounded Socratic tutor turn.
//
// POST body: {
//   concept:          string,   // the concept tag the student just missed on
//   item_explanation: string,   // the item's verified Explanation (the grounding)
//   mastery_bundle:   {...},    // focus + weakest-first prerequisites for `concept`
//   recent_history?:  {...},    // optional small revlog summary ("missed this twice")
//   user_message?:    string,   // the student's reply ("" on the first, auto-fired turn)
//   answer?:          string,   // OPTIONAL, check-only: item Answer for the post-check
// }
// Returns:   { turns: [{ role, content }], surfaced_prerequisite?, giveaway_blocked? }
//
// Design (notes/M3_tutor_design.md §1 bundle, §3 endpoint + guardrails):
//
//   1. GROUNDING CHECK (hard fail) — a turn with NO item_explanation has nothing
//      to ground on, so it is REFUSED before the model is ever called (the C1
//      "no grounding → refuse" rule applied to dialogue, §3.2). The model never
//      reasons from parametric memory.
//   2. State-grounded Socratic prompt — the tutor is grounded ONLY in the
//      client-supplied item_explanation + mastery_bundle. It surfaces the
//      weakest UNMASTERED prerequisite (mastery_bundle.prerequisites, weakest
//      -first) with a leading question and at most a partial hint (§3.3).
//   3. ANSWER-GIVEAWAY GUARDRAIL (server-side, §3.3) — (a) the system prompt
//      forbids emitting the item's answer/answer key; (b) the item `Answer` is
//      deliberately NOT sent as grounding (only `Explanation`); (c) an OPTIONAL,
//      rule-based post-check rejects/flags a turn that echoes a supplied answer
//      string (`giveaway_blocked=true`) — defense in depth, mirroring C1's
//      rule-based-first gate. This is enforced server-side so the client cannot
//      loosen it.
//
// This function NEVER writes to any DB. It does NOT retrieve corpus chunks (it is
// grounded in the client-supplied explanation + mastery state, not the corpus),
// so unlike generate/index.ts it needs no Supabase client, embedder, or retrieval.
// It reuses ONLY makeGenerator() from the shared provider abstraction.

import { handlePreflight, json } from "../_shared/http.ts";
import { makeGenerator, type Message } from "../_shared/provider.ts";

/** One concept node's mastery state — mirrors the core ConceptState (§2.2). */
interface ConceptState {
  concept?: string;
  section?: string;
  memory?: number;
  card_count?: number;
  scored_card_count?: number;
  sufficient_data?: boolean;
  mastered?: boolean;
  has_cards?: boolean;
}

/** The mastery bundle for the missed concept — focus + weakest-first prereqs. */
interface MasteryBundle {
  focus?: ConceptState;
  prerequisites?: ConceptState[];
}

interface TutorRequest {
  concept?: string;
  item_explanation?: string;
  mastery_bundle?: MasteryBundle;
  recent_history?: unknown;
  user_message?: string;
  // Check-only: the item Answer, used ONLY by the giveaway post-check below.
  // NEVER placed in a prompt (the model is grounded in Explanation, not Answer).
  answer?: string;
}

/** One dialogue turn returned to the client. */
interface Turn {
  role: "assistant";
  content: string;
}

/** A concept's short display name (last "::" segment), for readable prose. */
function conceptLabel(tag: string): string {
  const parts = tag.split("::").filter((p) => p.length > 0);
  const leaf = parts.length > 0 ? parts[parts.length - 1] : tag;
  return leaf.replace(/_/g, " ").trim() || tag;
}

/**
 * The weakest UNMASTERED prerequisite with cards — the lever the tutor surfaces
 * (§3.3, §5). The bundle is already ordered weakest-first, so the first prereq
 * that is unmastered AND has cards is the headline "thing holding you back".
 * Returns null when nothing qualifies (the tutor then works from the focus concept).
 */
function weakestPrerequisite(bundle: MasteryBundle | undefined): ConceptState | null {
  const prereqs = bundle?.prerequisites ?? [];
  for (const p of prereqs) {
    if (p.has_cards && !p.mastered) return p;
  }
  return null;
}

// Render the mastery bundle into compact text the model can reason over WITHOUT
// exposing it as facts to state. It is context about the *learner*, not the item.
function describeMastery(bundle: MasteryBundle | undefined): string {
  if (!bundle) return "(no mastery data supplied)";
  const lines: string[] = [];
  const focus = bundle.focus;
  if (focus?.concept) {
    const mem = Math.round(focus.memory ?? 0);
    const masteredWord = focus.mastered ? "mastered" : "not yet mastered";
    const detail = focus.sufficient_data
      ? `Memory ${mem}/100, ${masteredWord}.`
      : `not enough review data yet.`;
    lines.push(`Focus concept: ${conceptLabel(focus.concept)} — ${detail}`);
  }
  const prereqs = bundle.prerequisites ?? [];
  if (prereqs.length === 0) {
    lines.push("Prerequisites: none recorded.");
  } else {
    lines.push("Prerequisites (weakest first):");
    for (const p of prereqs) {
      if (!p.concept) continue;
      const status = !p.has_cards
        ? "no cards studied"
        : !p.sufficient_data
          ? "insufficient data"
          : p.mastered
            ? "mastered"
            : `weak (Memory ${Math.round(p.memory ?? 0)}/100)`;
      lines.push(`  - ${conceptLabel(p.concept)}: ${status}`);
    }
  }
  return lines.join("\n");
}

// System prompt that ENFORCES the grounding + no-giveaway contract server-side
// (§3.2, §3.3). The tutor reasons only from the supplied explanation + mastery
// state, surfaces the weak prerequisite Socratically, and never states the answer.
function buildMessages(
  conceptTag: string,
  explanation: string,
  bundle: MasteryBundle | undefined,
  weak: ConceptState | null,
  recentHistory: unknown,
  userMessage: string,
): Message[] {
  const lever = weak?.concept
    ? `The weakest unmastered prerequisite is "${conceptLabel(weak.concept)}". ` +
      `Open there: probe whether the student is solid on it, because it is the ` +
      `likely root of this miss.`
    : `No single weak prerequisite stands out; probe the student's reasoning on ` +
      `the focus concept itself to locate the gap.`;

  const historyNote =
    recentHistory && typeof recentHistory === "object"
      ? "\n\nRECENT HISTORY (learner-specific, for empathy/relevance only, not to be " +
        `quoted verbatim):\n${JSON.stringify(recentHistory)}`
      : "";

  const system =
    "You are a Socratic MCAT tutor. A student just missed an item on the " +
    `concept "${conceptLabel(conceptTag)}". Your job is to help them find the ` +
    "gap themselves — NOT to give the answer.\n\n" +
    "GROUNDING (hard rule): reason ONLY from the VERIFIED EXPLANATION and the " +
    "STUDENT MASTERY STATE below. Do not introduce outside facts and do not " +
    "invent content that is not supported by the explanation.\n\n" +
    "NEVER REVEAL THE ANSWER (hard rule): do not state, spell out, or strongly " +
    "imply the correct answer or answer key. Ask a leading question and give at " +
    "most a partial hint. If the student asks you to just tell them the answer, " +
    "decline and redirect them with a guiding question instead.\n\n" +
    "SURFACE THE WEAK PREREQUISITE: " +
    lever +
    "\n\nSTYLE: warm, brief (2-4 sentences), one focused question per turn.\n\n" +
    "VERIFIED EXPLANATION (grounding — reason from it, do NOT quote the answer " +
    `out of it):\n${explanation}\n\n` +
    `STUDENT MASTERY STATE:\n${describeMastery(bundle)}` +
    historyNote;

  const opening =
    "The student has just missed this item and has not said anything yet. Open " +
    "the tutoring turn: acknowledge the miss briefly, then ask ONE Socratic " +
    "question that probes the weak prerequisite (or the focus concept). Do not " +
    "reveal the answer.";

  return [
    { role: "system", content: system },
    { role: "user", content: userMessage.trim() || opening },
  ];
}

// OPTIONAL rule-based giveaway post-check (defense in depth, §3.3). If the client
// supplied the item `answer` for check-only, reject a reply that echoes it. Kept
// deliberately conservative (normalized substring on a non-trivial answer) so it
// mirrors C1's rule-based-first gate without over-blocking short answers.
function normalize(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}

function leaksAnswer(reply: string, answer: string | undefined): boolean {
  if (!answer) return false;
  const normAnswer = normalize(answer);
  // Too short to safely substring-match (e.g. "A", "7") — skip to avoid false positives.
  if (normAnswer.length < 4) return false;
  return normalize(reply).includes(normAnswer);
}

Deno.serve(async (req: Request): Promise<Response> => {
  const preflight = handlePreflight(req);
  if (preflight) return preflight;

  if (req.method !== "POST") {
    return json({ error: "method not allowed" }, 405);
  }

  let body: TutorRequest;
  try {
    body = await req.json();
  } catch {
    return json({ error: "invalid JSON body" }, 400);
  }

  const conceptTag = (body.concept ?? "").trim();
  const explanation = (body.item_explanation ?? "").trim();
  const bundle = body.mastery_bundle;
  const userMessage = body.user_message ?? "";

  // --- GROUNDING CHECK (hard fail) ----------------------------------------
  // No verified explanation => nothing to ground the tutor on => REFUSE before
  // the model is ever called (§3.2 — the C1 "no grounding → refuse" rule).
  if (explanation.length === 0) {
    return json(
      {
        status: "refused",
        reason: "no_grounding",
        message:
          "No verified explanation supplied. The tutor is grounded in the item's " +
          "explanation + the student's mastery state; it will not run ungrounded.",
      },
      422,
    );
  }

  const weak = weakestPrerequisite(bundle);

  // --- State-grounded Socratic turn ---------------------------------------
  let replyText: string;
  try {
    const generator = makeGenerator();
    const result = await generator.complete(
      buildMessages(conceptTag, explanation, bundle, weak, body.recent_history, userMessage),
    );
    replyText = result.text.trim();
  } catch (err) {
    // With no provider configured the stub throws here — surfaced honestly so the
    // tutor never fabricates a "successful" turn. The client degrades cleanly.
    return json(
      {
        status: "error",
        reason: "generator_unavailable",
        message: String(err instanceof Error ? err.message : err),
      },
      503,
    );
  }

  if (replyText.length === 0) {
    return json(
      { status: "error", reason: "empty_reply", message: "The tutor produced no reply." },
      502,
    );
  }

  // --- ANSWER-GIVEAWAY POST-CHECK (optional, defense in depth) -------------
  const giveawayBlocked = leaksAnswer(replyText, body.answer);
  if (giveawayBlocked) {
    // Replace the leaking turn with a safe redirect rather than surfacing the leak.
    replyText =
      "Let's not jump to the answer yet — walk me through your reasoning first. " +
      (weak?.concept
        ? `What do you remember about ${conceptLabel(weak.concept)}?`
        : "What made you rule the other options in or out?");
  }

  const turns: Turn[] = [{ role: "assistant", content: replyText }];
  return json({
    turns,
    surfaced_prerequisite: weak?.concept ?? null,
    giveaway_blocked: giveawayBlocked,
  });
});
