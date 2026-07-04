-- Synapse service — concept-scoped hybrid retrieval SQL function.
--
-- Enforces the retrieval contract (M2 design §4.2) in the database so both the
-- retrieve and generate Edge Functions get identical behavior:
--   1. FILTER by concept tag(s) first  (concept_tags @> match concept_tags)
--   2. THEN vector-rank the survivors  (order by embedding cosine distance)
--
-- Returns cosine SIMILARITY (1 - distance) so callers can threshold on a
-- higher-is-closer value. Uses the <=> cosine-distance operator, matching the
-- HNSW vector_cosine_ops index in 0001_init_corpus.sql. pgvector lives in the
-- `extensions` schema (0001), which is not on the search_path when this SQL
-- function body is parsed at CREATE time, so the operator is schema-qualified
-- via OPERATOR(extensions.<=>) to resolve it (avoids SQLSTATE 42883).

create or replace function public.match_corpus_chunks(
    concept_tags     text[],
    query_embedding  extensions.vector(1536),
    match_count      integer default 5
)
returns table (
    id             text,
    concept_tags   text[],
    aamc_category  text,
    text           text,
    source_title   text,
    source_section text,
    source_anchor  text,
    source_license text,
    similarity     real
)
language sql
stable
as $$
    select
        c.id,
        c.concept_tags,
        c.aamc_category,
        c.text,
        c.source_title,
        c.source_section,
        c.source_anchor,
        c.source_license,
        1 - (c.embedding OPERATOR(extensions.<=>) query_embedding) as similarity
    from public.corpus_chunks as c
    -- Step 1: structured concept filter FIRST. @> = array containment: the row's
    -- concept_tags must include (at least) the requested concept_tags.
    where c.concept_tags @> match_corpus_chunks.concept_tags
      and c.embedding is not null
    -- Step 2: vector rank within the concept-scoped survivors.
    order by c.embedding OPERATOR(extensions.<=>) query_embedding
    limit match_count;
$$;

comment on function public.match_corpus_chunks is
    'Concept-scoped hybrid retrieval: filter by concept_tags first, then '
    'vector-rank by cosine distance. Returns cosine similarity (higher=closer).';
