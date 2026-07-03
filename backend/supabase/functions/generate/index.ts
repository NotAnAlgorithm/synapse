// Edge Function: generate — the C1 grounded-generation pipeline SHAPE.
//
// POST body: { concept_tag: string, instruction?: string, top_k?: number }
// Returns:   a DRAFT for human review (status "draft"), never an approved item.
//
// Pipeline (M2 design §4.1-§4.3; PRD C1 — "ground the AI or don't ship it"):
//
//   1. Concept-scoped retrieval  — filter by concept tag first, then vector-rank.
//   2. GROUNDING CHECK (hard fail) — if retrieval returns NO chunk, REFUSE. No
//      chunk means no grounding; the model is never asked to invent.
//   3. Retrieval-augmented draft — the Generator is constrained by a system
//      prompt to transform ONLY the retrieved chunk text (grounding is in the
//      prompt contract, not trusted to the model).
//   4. RULE-BASED FLAW CHECK (hard fail) — deterministic item-writing rules
//      (flaw_check.ts), NOT an LLM judge. Structural defects are rejected before
//      any human sees the draft.
//   5. Return a DRAFT + a citation for HUMAN review. The service never
//      auto-approves and never writes to any collection. Approved content lands
//      client-side via add_note (M2 design §4.6, §5) — NOT here.
//
// EVERY returned draft carries a citation (100% of items — PRD C1 success bar).

import { createClient } from "https://esm.sh/@supabase/supabase-js@2";
import { handlePreflight, json } from "../_shared/http.ts";
import { makeEmbedder, makeGenerator, type Message } from "../_shared/provider.ts";
import { retrieve, type RetrievedChunk } from "../_shared/retrieval.ts";
import { checkMcq, type McqDraft } from "../_shared/flaw_check.ts";

const SUPABASE_URL = Deno.env.get("SUPABASE_URL") ?? "";
const SERVICE_ROLE_KEY = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY") ?? "";

interface GenerateRequest {
  concept_tag?: string;
  instruction?: string;
  top_k?: number;
}

/** The citation rendered into the note's user-visible Grounding field (§4.4). */
interface Citation {
  chunk_id: string;
  title: string;
  section: string;
  anchor: string;
  license: string;
}

function citationOf(chunk: RetrievedChunk): Citation {
  return {
    chunk_id: chunk.id,
    title: chunk.source_title,
    section: chunk.source_section,
    anchor: chunk.source_anchor,
    license: chunk.source_license,
  };
}

// System prompt that ENFORCES grounding: the model may only transform the
// provided source text; it must not add facts. This is the architectural
// expression of the hard non-goal (PRD Principle 4 / §11).
function buildMessages(
  conceptTag: string,
  instruction: string,
  chunks: RetrievedChunk[],
): Message[] {
  const grounding = chunks
    .map((c, i) => `[${i + 1}] (${c.id}) ${c.text}`)
    .join("\n");
  return [
    {
      role: "system",
      content:
        "You are a grounded MCAT item drafter. Transform ONLY the SOURCE FACTS " +
        "below into a single multiple-choice item. Do not introduce any fact " +
        "that is not stated in the SOURCE FACTS. Write four options with exactly " +
        "one defensibly-correct answer, plausible distractors, a single clear " +
        "stem, and an explanation drawn only from the SOURCE FACTS. Return JSON " +
        `with fields: stem, options (array), answerIndex (0-based), explanation.\n\n` +
        `CONCEPT: ${conceptTag}\n\nSOURCE FACTS:\n${grounding}`,
    },
    {
      role: "user",
      content: instruction || "Draft one application-level item for this concept.",
    },
  ];
}

// Parse the model's JSON draft into the flaw-checker shape. Defensive: a
// malformed draft is treated as a failed generation (never shipped).
function parseDraft(text: string): McqDraft | null {
  try {
    const obj = JSON.parse(text) as Partial<McqDraft>;
    if (
      typeof obj.stem === "string" &&
      Array.isArray(obj.options) &&
      typeof obj.answerIndex === "number"
    ) {
      return {
        stem: obj.stem,
        options: obj.options as string[],
        answerIndex: obj.answerIndex,
        explanation: typeof obj.explanation === "string" ? obj.explanation : undefined,
      };
    }
  } catch {
    // fallthrough
  }
  return null;
}

Deno.serve(async (req: Request): Promise<Response> => {
  const preflight = handlePreflight(req);
  if (preflight) return preflight;

  if (req.method !== "POST") {
    return json({ error: "method not allowed" }, 405);
  }

  let body: GenerateRequest;
  try {
    body = await req.json();
  } catch {
    return json({ error: "invalid JSON body" }, 400);
  }

  const conceptTag = (body.concept_tag ?? "").trim();
  const instruction = (body.instruction ?? "").trim();
  const topK = Math.min(Math.max(body.top_k ?? 4, 1), 20);
  if (conceptTag.length === 0) {
    return json({ error: "concept_tag is required (generation is concept-scoped)" }, 400);
  }

  const client = createClient(SUPABASE_URL, SERVICE_ROLE_KEY);
  const embedder = makeEmbedder();
  const generator = makeGenerator();

  // --- Step 1: concept-scoped retrieval -----------------------------------
  let chunks: RetrievedChunk[];
  try {
    chunks = await retrieve(client, embedder, {
      conceptTags: [conceptTag],
      query: instruction || conceptTag,
      topK,
    });
  } catch (err) {
    return json({ error: String(err instanceof Error ? err.message : err) }, 500);
  }

  // --- Step 2: GROUNDING CHECK (hard fail) --------------------------------
  // No retrieved chunk => no grounding => REFUSE. The model is never called.
  if (chunks.length === 0) {
    return json(
      {
        status: "refused",
        reason: "no_grounding",
        message:
          `No corpus grounding for ${conceptTag}. Generation refused (hard ` +
          `grounding gate; no ungrounded AI).`,
      },
      422,
    );
  }

  // --- Step 3: retrieval-augmented draft ----------------------------------
  let draftText: string;
  try {
    const result = await generator.complete(buildMessages(conceptTag, instruction, chunks));
    draftText = result.text;
  } catch (err) {
    // With no provider configured the stub throws here — surfaced honestly so
    // the pipeline never fabricates a "successful" ungrounded draft.
    return json(
      { status: "error", reason: "generator_unavailable", message: String(err instanceof Error ? err.message : err) },
      503,
    );
  }

  const draft = parseDraft(draftText);
  if (!draft) {
    return json(
      { status: "rejected", reason: "unparseable_draft", message: "Model output was not a valid item." },
      422,
    );
  }

  // --- Step 4: RULE-BASED FLAW CHECK (deterministic, hard fail) -----------
  const flaw = checkMcq(draft);
  if (!flaw.ok) {
    return json(
      {
        status: "rejected",
        reason: "flaw_check_failed",
        findings: flaw.findings,
        message: "Draft failed the rule-based item-writing checker; not queued for review.",
      },
      422,
    );
  }

  // --- Step 5: return a DRAFT for HUMAN review (never auto-approved) -------
  // Citation on 100% of items (PRD C1). The draft is NOT written to any
  // collection; a human reviewer approves/edits/rejects, and only approved
  // content lands client-side via add_note (M2 design §4.6, §5).
  return json({
    status: "draft",
    review_required: true,
    concept_tag: conceptTag,
    item: draft,
    // Citation is required and always present; ties the draft to its grounding.
    citation: citationOf(chunks[0]),
    grounding_chunk_ids: chunks.map((c) => c.id),
  });
});
