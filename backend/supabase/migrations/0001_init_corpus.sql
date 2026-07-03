-- Synapse service — initial schema: pgvector + the vetted-source corpus.
--
-- This migration stands up ONLY the corpus side of the service (server-owned,
-- not per-user): the extension, the corpus_chunks table, and its vector index.
-- Per-user tables + RLS are stubbed in 0002_rls_stubs.sql because the auth model
-- (Supabase Auth vs the linked Synapse identity) is still TBD (M2 design §3, §10).
--
-- Infra defaults chosen here (both listed as open questions for the owner):
--   * embedding dimension = 1536  (matches common 1536-dim embedding models;
--     change EMBEDDING_DIM everywhere if the chosen Embedder differs).
--   * vector index = HNSW with cosine distance (vector_cosine_ops) — the
--     recommended default for small/medium curated corpora with good recall and
--     no train step, cheaper to maintain than IVFFlat for our corpus size.

-- pgvector: vector type + ANN index operators. On Supabase the extension lives
-- in the dedicated `extensions` schema by convention.
create extension if not exists vector with schema extensions;

-- ---------------------------------------------------------------------------
-- corpus_chunks: one row per atomic, openly-licensed grounding fact.
-- Mirrors corpus/seed.jsonl one-to-one (see corpus/README.md for the contract).
-- This table is server-owned reference data; it is NOT per-user and carries no
-- RLS user predicate (read access is granted to the service role / functions).
-- ---------------------------------------------------------------------------
create table if not exists public.corpus_chunks (
    -- Stable slug from the JSONL `id`; also the grounding handle stored on
    -- generated notes' lineage rows in the core (M2 design §4.6).
    id             text primary key,

    -- Concept scoping: the M0/M1 tag convention concept::<section>::<id>.
    -- Retrieval filters on this array FIRST, then vector-ranks (M2 design §4.2).
    concept_tags   text[]        not null,

    -- Coarse AAMC content-category code (e.g. '1A', '4B', '7A') for reporting.
    aamc_category  text          not null,

    -- The atomic fact the generator is constrained to transform.
    text           text          not null,

    -- Citation metadata, flattened from the JSONL `source` object. Rendered into
    -- the user-visible Grounding field on generated notes (M2 design §4.4).
    source_title   text          not null,
    source_section text          not null,
    source_anchor  text          not null,
    -- Licensing gate lives in the app/validator; the CHECK is a last-resort
    -- guard so copyright-restricted rows can never be inserted here.
    source_license text          not null
        check (source_license in ('CC-BY-4.0', 'Synapse-Original')),

    -- Embedding of `text` produced by the configured Embedder. NULL until the
    -- ingest step backfills it. Keep the dimension in sync with the Embedder.
    embedding      extensions.vector(1536),

    created_at     timestamptz   not null default now(),
    updated_at     timestamptz   not null default now()
);

-- Concept-scoped retrieval filters on concept_tags first; a GIN index keeps the
-- array-containment predicate (concept_tags @> ARRAY[...]) fast.
create index if not exists corpus_chunks_concept_tags_idx
    on public.corpus_chunks using gin (concept_tags);

-- ANN vector index for the rank step. HNSW + cosine (see header note). Only rows
-- with a non-null embedding participate; NULLs are simply skipped by the index.
create index if not exists corpus_chunks_embedding_hnsw_idx
    on public.corpus_chunks using hnsw (embedding extensions.vector_cosine_ops);

comment on table public.corpus_chunks is
    'Vetted, openly-licensed grounding facts (OpenStax CC-BY or Synapse-original). '
    'Server-owned reference data; source of grounding for all AI generation.';
