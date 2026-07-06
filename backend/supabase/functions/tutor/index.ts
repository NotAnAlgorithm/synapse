// Edge Function: tutor — the C2 state-grounded, conversational Socratic tutor.
//
// This is a STATELESS-PER-TURN endpoint: the CLIENT carries the whole thread and
// re-sends it every turn (there is no server-side thread store). Each turn the
// server rebuilds [system, ...messages], runs the provider once, and returns the
// assistant reply.
//
// POST body (the client sends this every turn): {
//   concept:          string,   // the concept:: tag the item is about
//   item_explanation: string,   // the item's verified Explanation (the grounding, REQUIRED)
//   item_question?:   string,   // the card question/stem, for context
//   item_answer?:     string,   // the correct answer
//   card_revealed:    boolean,  // is the answer side CURRENTLY shown in the reviewer?
//   mastery_bundle:   {...},    // focus + weakest-first prerequisites for `concept`
//   messages:         [{role:"user"|"assistant", content}, ...]  // conversation so far (excl. system)
// }
// Returns: { reply: string, giveaway_blocked?: boolean }
//
// Design (notes/M3_tutor_design.md §3 grounding + server-side giveaway guardrail):
//
//   1. GROUNDING CHECK (hard fail) — a turn with NO item_explanation has nothing
//      to ground on, so it is REFUSED before the model is ever called (the C1
//      "no grounding → refuse" rule applied to dialogue, §3.2). The model never
//      reasons from parametric memory.
//   2. State-grounded, card-state-conditioned prompt — the tutor is grounded
//      ONLY in the client-supplied item_explanation + mastery_bundle (+
//      item_question for context). item_answer is placed in the prompt ONLY when
//      card_revealed is true (the student has already seen it).
//   3. CARD-STATE-CONDITIONED ANSWER-GIVEAWAY GUARDRAIL (server-side, §3.3):
//      * card_revealed === false — the student has NOT seen the answer; the
//        tutor NEVER reveals or hints it (stays Socratic, steers toward the
//        weakest unmastered prerequisite). The rule-based answer-giveaway
//        post-check runs; if it trips, we regenerate once, then fall back to a
//        safe redirect, and set giveaway_blocked.
//      * card_revealed === true — the student has already seen the correct
//        answer; the tutor MAY explain/discuss it directly to clear up
//        confusion, grounded in the explanation. The giveaway guardrail is moot
//        and is NOT run/enforced.
//
// This function NEVER writes to any DB and does NOT retrieve corpus chunks (it is
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

/** The mastery bundle for the concept — focus + weakest-first prereqs. */
interface MasteryBundle {
  focus?: ConceptState;
  prerequisites?: ConceptState[];
}

/** One conversation turn the client carries (system is added server-side). */
interface ChatMessage {
  role: "user" | "assistant";
  content: string;
}

interface TutorRequest {
  concept?: string;
  item_explanation?: string;
  item_question?: string;
  // The correct answer. Placed in the prompt ONLY when card_revealed is true;
  // otherwise used only by the giveaway post-check (never in the prompt).
  item_answer?: string;
  card_revealed?: boolean;
  mastery_bundle?: MasteryBundle;
  messages?: ChatMessage[];
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

// Render the mastery bundle into compact text the model can reason over. It is
// context about the *learner*, not the item.
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

// System prompt that ENFORCES the grounding + card-state-conditioned no-giveaway
// contract server-side (§3.2, §3.3). The tutor reasons only from the supplied
// explanation + mastery state (+ question for context). When the card is NOT
// revealed it must stay Socratic and never state the answer; when it IS revealed
// the student has already seen the answer, so the tutor may discuss it directly.
function buildSystemPrompt(
  conceptTag: string,
  explanation: string,
  question: string,
  answer: string,
  cardRevealed: boolean,
  bundle: MasteryBundle | undefined,
  weak: ConceptState | null,
): string {
  const lever = weak?.concept
    ? `The weakest unmastered prerequisite is "${conceptLabel(weak.concept)}". ` +
      `Steer there: probe whether the student is solid on it, because it is the ` +
      `likely root of their confusion.`
    : `No single weak prerequisite stands out; probe the student's reasoning on ` +
      `the focus concept itself to locate the gap.`;

  const questionNote = question
    ? `\n\nITEM QUESTION (context — what the student was asked):\n${question}`
    : "";

  const guardrail = cardRevealed
    ? "CARD STATE: the student has ALREADY SEEN the correct answer (given below). " +
      "You MAY explain and discuss the answer directly, grounded in the VERIFIED " +
      "EXPLANATION, to clear up their confusion. Still teach — connect the answer " +
      "back to the underlying concept and the weak prerequisite; do not merely " +
      "restate it.\n\n" +
      `CORRECT ANSWER (the student has seen this):\n${answer || "(not supplied)"}`
    : "CARD STATE: the student has NOT yet seen the answer. NEVER REVEAL THE " +
      "ANSWER (hard rule): do not state, spell out, or strongly imply the correct " +
      "answer or answer key. Ask a leading question and give at most a partial " +
      "hint. If the student asks you to just tell them the answer, decline and " +
      "redirect them with a guiding question instead.";

  return (
    "You are a Socratic MCAT tutor having a conversation with a student about an " +
    `item on the concept "${conceptLabel(conceptTag)}". Help them find and close ` +
    "the gap in their understanding.\n\n" +
    "GROUNDING (hard rule): reason ONLY from the VERIFIED EXPLANATION, the ITEM " +
    "QUESTION, and the STUDENT MASTERY STATE below. Do not introduce outside " +
    "facts and do not invent content that is not supported by the explanation.\n\n" +
    guardrail +
    "\n\nSURFACE THE WEAK PREREQUISITE: " +
    lever +
    "\n\nSTYLE: warm and conversational, brief (2-4 sentences), one focused " +
    "question or point per turn. Respond directly to what the student says.\n\n" +
    "VERIFIED EXPLANATION (grounding — reason from it):\n" +
    explanation +
    questionNote +
    `\n\nSTUDENT MASTERY STATE:\n${describeMastery(bundle)}`
  );
}

// The implicit opening user turn, seeded when the client sends no messages, so
// the assistant produces a proactive Socratic opener rather than an empty turn.
const OPENING_USER_TURN =
  "I just reviewed this — help me understand what I might be missing.";

// Assemble the provider messages: [system, ...conversation]. When the client
// sends no messages, seed a single implicit opening user turn.
function buildMessages(system: string, messages: ChatMessage[]): Message[] {
  const convo: ChatMessage[] =
    messages.length > 0 ? messages : [{ role: "user", content: OPENING_USER_TURN }];
  const out: Message[] = [{ role: "system", content: system }];
  for (const m of convo) {
    const content = (m?.content ?? "").trim();
    if (content.length === 0) continue;
    // Only user/assistant turns are carried; anything else is ignored defensively.
    if (m.role === "user" || m.role === "assistant") {
      out.push({ role: m.role, content });
    }
  }
  return out;
}

// Rule-based giveaway post-check (defense in depth, §3.3). Only meaningful when
// the card is NOT revealed. Kept deliberately conservative (normalized substring
// on a non-trivial answer) so it mirrors C1's rule-based-first gate without
// over-blocking short answers.
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

// A safe, answer-free redirect used when a hidden-answer turn leaks and the
// regeneration also leaks (or fails). Steers back to the weak prerequisite.
function safeRedirect(weak: ConceptState | null): string {
  return (
    "Let's not jump to the answer yet — walk me through your reasoning first. " +
    (weak?.concept
      ? `What do you remember about ${conceptLabel(weak.concept)}?`
      : "What made you rule the other options in or out?")
  );
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
  const question = (body.item_question ?? "").trim();
  const answer = (body.item_answer ?? "").trim();
  const cardRevealed = body.card_revealed === true;
  const bundle = body.mastery_bundle;
  const messages = Array.isArray(body.messages) ? body.messages : [];

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
  const system = buildSystemPrompt(
    conceptTag,
    explanation,
    question,
    answer,
    cardRevealed,
    bundle,
    weak,
  );

  const generator = makeGenerator();

  // Generate one assistant turn from [system, ...messages].
  const generateReply = async (): Promise<string> => {
    const result = await generator.complete(buildMessages(system, messages));
    return result.text.trim();
  };

  let replyText: string;
  try {
    replyText = await generateReply();
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

  // --- ANSWER-GIVEAWAY POST-CHECK (only when the answer is HIDDEN) ---------
  // When the card is revealed the student has already seen the answer, so the
  // guardrail is moot and is not enforced (§3.3). When it is hidden, run the
  // rule-based check; on a leak, regenerate once, then fall back to a safe
  // redirect. `giveaway_blocked` is reported to the client either way.
  let giveawayBlocked = false;
  if (!cardRevealed && leaksAnswer(replyText, answer)) {
    giveawayBlocked = true;
    try {
      const retry = await generateReply();
      if (retry.length > 0 && !leaksAnswer(retry, answer)) {
        replyText = retry;
      } else {
        replyText = safeRedirect(weak);
      }
    } catch {
      replyText = safeRedirect(weak);
    }
  }

  return json({ reply: replyText, giveaway_blocked: giveawayBlocked });
});
