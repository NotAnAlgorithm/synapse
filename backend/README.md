# Synapse service layer

The hosted grounded-AI service for **Synapse** (an MCAT study app forked from
Anki). This is a **separate repository** from the Anki fork on purpose: the Rust
core makes no LLM/network calls beyond sync, so all AI orchestration —
retrieval, generation, the quality gates, and (later) the tutor / placement /
calibration services — lives here.

The governing design is `notes/M2_service_layer_design.md` in the Anki fork; this
repo implements the **M2 skeleton** of it (Phase 2, steps 1-2). Read that doc for
the full rationale and the open decisions; this README covers what is scaffolded
and how to bring it up.

> **Status:** structure-only scaffold. No provider keys, no Supabase project, and
> nothing is deployed or run. The LLM/embedding provider is a `stub` that
> *refuses* rather than fabricates, so the pipeline can never accidentally ship
> ungrounded content before it is wired.

## Architecture (topology = hosted, Supabase)

Locked topology (M2 design §2, Option B — hosted cloud): a single Synapse-operated
backend owns the vetted corpus + vector index, brokers LLM calls, and runs the
quality gates. Clients call it over HTTPS; the core never proxies AI.

- **Supabase Postgres + pgvector** — stores the corpus (`corpus_chunks`) and its
  embeddings, and the per-user service data (tutor threads, placement sessions,
  F3 calibration tuples). Per-user isolation via **RLS** (M2 design §5.5).
- **Supabase Edge Functions (Deno/TS)** — the request surface:
  - `retrieve` — concept-scoped hybrid retrieval (filter by concept tag first,
    then vector-rank). This is the lowest-risk spike to stand up first
    (M2 design §7, step 1).
  - `generate` — the C1 grounded-generation pipeline (below).
- **Provider abstraction** (`supabase/functions/_shared/provider.ts`) — a thin
  `Generator` + `Embedder` behind interfaces so the provider/model/prompts are
  **config, not code** (M2 design §4.1). The owner's provider slots in here.

```
client shell (Qt/Kotlin)  --HTTPS-->  Edge Functions (retrieve, generate)
                                              |
                                    Postgres + pgvector (corpus_chunks, per-user)
                                              |
                                       LLM provider (behind Generator/Embedder)
```

```
corpus/                         vetted, openly-licensed grounding facts
  README.md                     JSONL record format + why atomic + retrieval
  seed.jsonl                    ~24 atomic records over the 4 seeded concepts
scripts/
  validate_corpus.py            standalone field + license validator (do not run)
  ingest.ts                     chunk -> embed -> upsert (structure only)
supabase/
  config.toml                   project + function declarations
  migrations/
    0001_init_corpus.sql        pgvector + corpus_chunks + GIN/HNSW indexes
    0002_rls_stubs.sql          per-user tables + RLS policy stubs (auth TBD)
    0003_match_fn.sql           match_corpus_chunks: filter-first, rank-second
  functions/
    _shared/                    provider abstraction, flaw checker, retrieval, http
    retrieve/index.ts           concept-scoped hybrid retrieval
    generate/index.ts           C1 pipeline
.env.example                    config template (NO secrets)
```

## The grounding + flaw + human gate (C1)

PRD Principle 4: *ground the AI or don't ship it.* PRD §11: ungrounded AI
generation is a **hard non-goal**. The `generate` function encodes that as three
gates, in this order (M2 design §4.3; PRD C1):

1. **Grounding check — hard fail.** Generation is retrieval-augmented: the prompt
   contains the retrieved corpus chunk(s), and the system instruction constrains
   the model to transform *only* that text. If concept-scoped retrieval returns
   **no chunk**, the request is **refused** — the model is never called. There is
   no code path from a client to ungrounded generation because the corpus lives
   only on the service.
2. **Rule-based flaw check — hard fail, deterministic.** A deterministic
   item-writing checker (`_shared/flaw_check.ts`) runs **before any human sees the
   draft**: no duplicate/defensibly-equivalent options, no "all/none of the
   above" abuse, no answer-giveaway length/absolute-term cues, a single clear
   stem, a valid single answer key. This is **code, not an LLM judge** — per the
   PRD, rule-based catches ~91% of flaws vs ~79% for an LLM judge, and AI MCQs
   carry higher rates of exactly these defects.
3. **Human review — required.** The function returns a **draft** (`status:
   "draft"`, `review_required: true`) with a citation. It is **never
   auto-approved and never written to any collection.** A human reviewer
   approves / edits / rejects; only approved items proceed.

Every returned draft carries a **citation** (100% of items, PRD C1 success bar),
plus the grounding chunk ids for auditability.

## How approved content lands back in the client (client-mediated)

The service **does not write to the collection.** Content lands through the normal
core path (M2 design §5, §4.6):

1. `generate` returns a reviewed-and-approved draft + its citation to the client.
2. The **client** builds a `Note` on the appropriate MCAT notetype, populates the
   `Grounding` field with the citation, and calls the existing **`add_note`** RPC
   inside a `CollectionOp`.
3. Card generation, concept-tag projection, and **sync** happen for free — the
   note reaches desktop and Android through the existing sync path.

No new core RPC is needed to land content; `add_note` + sync is the write-back.
The service stores only drafts/derived data, never `.anki2` files.

## Corpus & licensing

`corpus/seed.jsonl` is a tiny seed set (~24 atomic records) aligned to the four
concepts the client already ships (`concept::biochem::amino_acid_charge`,
`concept::biochem::enzyme_kinetics`, `concept::physics::circuits_ohms_law`,
`concept::psych::operant_conditioning`). **Every record is owned or openly
licensed** — OpenStax (CC-BY-4.0) paraphrase or Synapse-original text. The
author-reference UWorld PDFs are copyright-restricted and **must never** enter the
corpus; `scripts/validate_corpus.py` enforces an allowed `license` on every
record and a `CHECK` constraint in the migration is a last-resort guard. See
`corpus/README.md` for the record contract.

## Next steps once Supabase keys / a provider are provided

Nothing below has been run. In order:

1. **Create the Supabase project** and link it: `supabase link --project-ref <ref>`.
   Copy `.env.example` to `.env` and fill `SUPABASE_URL`, `SUPABASE_ANON_KEY`, and
   `SUPABASE_SERVICE_ROLE_KEY`.
2. **Choose the embedding dimension** to match the provider's embedding model. If
   it is not 1536, update `vector(1536)` in `supabase/migrations/0001_init_corpus.sql`
   *and* `0003_match_fn.sql` *and* `SYNAPSE_EMBEDDING_DIM` in `.env` — they must
   agree. (See open questions.)
3. **Apply migrations:** `supabase db reset` (local) or `supabase db push`
   (linked). This enables pgvector and creates `corpus_chunks`, the indexes, the
   per-user tables (RLS enabled, deny-by-default), and `match_corpus_chunks`.
4. **Wire the provider:** implement the real `Generator`/`Embedder` adapters in
   `supabase/functions/_shared/provider.ts` (add a `case` in `makeGenerator` /
   `makeEmbedder`), set `SYNAPSE_PROVIDER` + model names, and set the API key as a
   function secret: `supabase secrets set SYNAPSE_PROVIDER_API_KEY=...`.
5. **Validate + ingest the corpus:**
   `python3 scripts/validate_corpus.py corpus/seed.jsonl`, then
   `deno run --allow-env --allow-read --allow-net scripts/ingest.ts corpus/seed.jsonl`
   to embed and upsert into `corpus_chunks`.
6. **Deploy + smoke-test the functions:** `supabase functions deploy retrieve`
   and `supabase functions deploy generate`. POST a concept to `retrieve` and
   confirm on-concept grounding comes back (the M2 §7 step-1 quality gate). POST
   to `generate` and confirm it returns a **draft** with a citation, or **refuses**
   for a concept with no corpus coverage.
7. **Un-stub RLS + auth** once the identity model is decided (M2 design §3):
   replace the commented policies in `0002_rls_stubs.sql` with the chosen
   per-user predicate before collecting any multi-user data.
8. **Stand up the human-review console** (separate surface): drafts from
   `generate` are queued, a reviewer approves/edits/rejects, and only approved
   items are handed to the client for `add_note`.

## Constraints carried from the design

- The base learning loop is **service-independent**; every service call is
  best-effort and degrades to an "AI unavailable" state (M2 design §9).
- The corpus never ships to the device; generation is centralized so the gates
  cannot be bypassed client-side (M2 design §6).
- Residual hallucination is monitored, not assumed solved: target defect rate
  ≤ the human item-writer baseline; gate harder if exceeded (M2 design §4.5).
