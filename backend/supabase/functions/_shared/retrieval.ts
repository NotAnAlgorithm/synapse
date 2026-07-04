// Concept-scoped hybrid retrieval, shared by the retrieve + generate functions.
//
// The contract (M2 design §4.2): FILTER by concept tag(s) first (structured,
// exact), THEN vector-rank the survivors. Concept-scoping both improves
// precision and guarantees the grounding stays on-topic for the concept being
// generated for. A request whose concept has no matching chunks returns [] —
// and the generate pipeline REFUSES on empty grounding (the hard grounding gate).

import type { Embedder } from "./provider.ts";

export interface RetrievedChunk {
  id: string;
  concept_tags: string[];
  aamc_category: string;
  text: string;
  source_title: string;
  source_section: string;
  source_anchor: string;
  source_license: string;
  /** Cosine similarity in [0, 1] from the vector rank step (higher = closer). */
  similarity: number;
}

export interface RetrieveParams {
  conceptTags: string[];
  query: string;
  topK: number;
}

// Minimal shape of the Supabase JS client method we use, to avoid importing the
// full SDK types at scaffold time. The real client is injected by the caller.
export interface RpcClient {
  // The real Supabase client's rpc() returns a thenable PostgrestFilterBuilder
  // (awaitable, but not a full Promise), so this is typed as PromiseLike so the
  // concrete SupabaseClient is assignable here without importing its full types.
  rpc(
    fn: string,
    args: Record<string, unknown>,
  ): PromiseLike<{ data: unknown; error: { message: string } | null }>;
}

/**
 * Concept-scoped hybrid retrieval:
 *   1. embed the query (Embedder abstraction),
 *   2. call the `match_corpus_chunks` SQL function which filters by
 *      concept_tags @> conceptTags FIRST, then orders by embedding distance.
 *
 * The filter-first / rank-second ordering is enforced in SQL (see the migration
 * companion `match_corpus_chunks` in 0003_match_fn.sql).
 */
export async function retrieve(
  client: RpcClient,
  embedder: Embedder,
  params: RetrieveParams,
): Promise<RetrievedChunk[]> {
  const { conceptTags, query, topK } = params;
  if (conceptTags.length === 0) {
    // No concept scope → no grounding. Never fall back to an unscoped search.
    return [];
  }

  const [queryEmbedding] = await embedder.embed([query]);

  const { data, error } = await client.rpc("match_corpus_chunks", {
    concept_tags: conceptTags,
    query_embedding: queryEmbedding,
    match_count: topK,
  });
  if (error) {
    throw new Error(`retrieval failed: ${error.message}`);
  }
  return (data as RetrievedChunk[]) ?? [];
}
