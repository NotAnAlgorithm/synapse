// Edge Function: retrieve — concept-scoped hybrid retrieval over corpus_chunks.
//
// POST body: { concept_tags: string[], query: string, top_k?: number }
// Returns:   { chunks: RetrievedChunk[] }
//
// This is the RAG-retrieval spike the phasing plan prototypes FIRST (M2 design
// §7, step 1): no generation, no auth beyond a dev token, no core changes — it
// directly tests that concept-scoped grounding is precise. Retrieval FILTERS by
// concept tag(s) first, then vector-ranks (via the match_corpus_chunks SQL fn).

import { createClient } from "https://esm.sh/@supabase/supabase-js@2";
import { handlePreflight, json } from "../_shared/http.ts";
import { makeEmbedder } from "../_shared/provider.ts";
import { retrieve } from "../_shared/retrieval.ts";

const SUPABASE_URL = Deno.env.get("SUPABASE_URL") ?? "";
// Server-side Supabase key: the new "secret" key (sb_secret_...) if set as a
// function secret, else the legacy service_role JWT that Supabase still
// auto-injects into deployed functions. Never in the repo. Interim auth is a
// shared dev token (M2 design §3); real per-user auth is required before any
// multi-user data is collected.
const SECRET_KEY =
  Deno.env.get("SUPABASE_SECRET_KEY") ?? Deno.env.get("SUPABASE_SERVICE_ROLE_KEY") ?? "";

interface RetrieveRequest {
  concept_tags?: string[];
  query?: string;
  top_k?: number;
}

Deno.serve(async (req: Request): Promise<Response> => {
  const preflight = handlePreflight(req);
  if (preflight) return preflight;

  if (req.method !== "POST") {
    return json({ error: "method not allowed" }, 405);
  }

  let body: RetrieveRequest;
  try {
    body = await req.json();
  } catch {
    return json({ error: "invalid JSON body" }, 400);
  }

  const conceptTags = body.concept_tags ?? [];
  const query = (body.query ?? "").trim();
  const topK = Math.min(Math.max(body.top_k ?? 5, 1), 20);

  if (conceptTags.length === 0) {
    return json({ error: "concept_tags is required (retrieval is concept-scoped)" }, 400);
  }
  if (query.length === 0) {
    return json({ error: "query is required" }, 400);
  }

  const client = createClient(SUPABASE_URL, SECRET_KEY);
  const embedder = makeEmbedder();

  try {
    const chunks = await retrieve(client, embedder, { conceptTags, query, topK });
    return json({ chunks });
  } catch (err) {
    return json({ error: String(err instanceof Error ? err.message : err) }, 500);
  }
});
