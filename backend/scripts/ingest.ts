// Ingest script: chunk corpus/seed.jsonl -> embed -> upsert into corpus_chunks.
//
// STRUCTURE ONLY — do NOT run in this environment (no keys, integrator runs it).
// Run locally once a provider + Supabase project exist:
//   deno run --allow-env --allow-read --allow-net scripts/ingest.ts corpus/seed.jsonl
//
// The corpus is already authored as ATOMIC records (one fact per line), so the
// "chunk" step is 1 record -> 1 chunk; the function is kept explicit so a future
// multi-fact source can be split here without touching the embed/upsert steps.

import { createClient } from "https://esm.sh/@supabase/supabase-js@2";
import { makeEmbedder } from "../supabase/functions/_shared/provider.ts";

interface CorpusRecord {
  id: string;
  concept_tags: string[];
  aamc_category: string;
  text: string;
  source: {
    title: string;
    section: string;
    anchor: string;
    license: string;
  };
}

/** A row ready to upsert into public.corpus_chunks (embedding filled below). */
interface ChunkRow {
  id: string;
  concept_tags: string[];
  aamc_category: string;
  text: string;
  source_title: string;
  source_section: string;
  source_anchor: string;
  source_license: string;
  embedding: number[];
}

const SUPABASE_URL = Deno.env.get("SUPABASE_URL") ?? "";
const SERVICE_ROLE_KEY = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY") ?? "";
const BATCH_SIZE = 32; // embed + upsert in modest batches

function parseJsonl(raw: string): CorpusRecord[] {
  return raw
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l.length > 0)
    .map((l) => JSON.parse(l) as CorpusRecord);
}

/**
 * Chunk one record. The seed corpus is atomic, so this is identity (1->1). Split
 * multi-fact source text here if a future record carries more than one fact.
 */
function chunkRecord(record: CorpusRecord): CorpusRecord[] {
  return [record];
}

async function main(): Promise<void> {
  const path = Deno.args[0] ?? "corpus/seed.jsonl";
  const raw = await Deno.readTextFile(path);
  const records = parseJsonl(raw).flatMap(chunkRecord);

  const client = createClient(SUPABASE_URL, SERVICE_ROLE_KEY);
  const embedder = makeEmbedder();

  let upserted = 0;
  for (let i = 0; i < records.length; i += BATCH_SIZE) {
    const batch = records.slice(i, i + BATCH_SIZE);
    // Embed the atomic fact text; dimension must match the vector column.
    const embeddings = await embedder.embed(batch.map((r) => r.text));

    const rows: ChunkRow[] = batch.map((r, j) => ({
      id: r.id,
      concept_tags: r.concept_tags,
      aamc_category: r.aamc_category,
      text: r.text,
      source_title: r.source.title,
      source_section: r.source.section,
      source_anchor: r.source.anchor,
      source_license: r.source.license,
      embedding: embeddings[j],
    }));

    // Upsert on the primary key so re-ingesting is idempotent.
    const { error } = await client.from("corpus_chunks").upsert(rows, { onConflict: "id" });
    if (error) {
      throw new Error(`upsert failed at batch starting ${i}: ${error.message}`);
    }
    upserted += rows.length;
    console.log(`upserted ${upserted}/${records.length}`);
  }

  console.log(`done: ${upserted} chunk(s) ingested from ${path}`);
}

if (import.meta.main) {
  await main();
}
