# Synapse vetted-source corpus

This directory holds the **vetted-source corpus** that grounds every AI item the
Synapse service produces. Nothing the model writes reaches a learner unless it
can be attributed to a chunk in this corpus (see the grounding gate in the repo
root `README.md` and `notes/M2_service_layer_design.md §4.3` in the Anki fork).

> **Licensing is non-negotiable.** Every record in this corpus must be
> **owned or openly licensed**: OpenStax (CC-BY 4.0) excerpts/paraphrase, or
> text authored originally for Synapse. The author-reference UWorld PDFs are
> **copyright-restricted and must never appear here** — not as text, not as a
> paraphrase, not as a citation anchor. `scripts/validate_corpus` enforces a
> `license` on every record; keep it honest.

## Why a curated corpus at all

The PRD's hard non-goal is *no ungrounded AI, ever* (`notes/prd.md` Principle 4,
§11). The corpus is the architectural expression of that rule: generation is
*retrieval-augmented transformation of this text*, never free recall from the
model's parameters. Because the corpus is small and curated (not web-scale) a
simple pgvector index over it is enough (`M2 design §4.2`).

## Record format (`seed.jsonl`)

The corpus is stored as **JSON Lines** — one JSON object per line, UTF-8, no
trailing commas, no surrounding array. Each line is one **atomic record**:

```json
{
  "id": "os-biochem-enzyme-km-competitive",
  "concept_tags": ["concept::biochem::enzyme_kinetics"],
  "aamc_category": "1A",
  "text": "A competitive inhibitor raises an enzyme's apparent Km while leaving Vmax unchanged, because it competes with substrate for the active site and can be outcompeted at high substrate concentration.",
  "source": {
    "title": "OpenStax Biology 2e",
    "section": "6.5 Enzymes",
    "anchor": "enzyme-inhibition",
    "license": "CC-BY-4.0"
  }
}
```

### Fields

| Field | Type | Meaning |
|---|---|---|
| `id` | string | Stable, unique, human-readable slug. Prefix by provenance: `os-*` for OpenStax-derived, `syn-*` for Synapse-original. Used as the chunk primary key and the grounding-lineage handle on generated notes. |
| `concept_tags` | string[] | One or more concept tags in the **exact** M0/M1 convention `concept::<section>::<id>` (underscores, no spaces). Must match tags the client already uses (`qt/aqt/synapse/provision.py`). At least one tag is required — this is what makes retrieval concept-scoped. |
| `aamc_category` | string | The AAMC content-category code the fact sits under (e.g. `1A`, `4B`, `7A`). Coarse, for reporting/coverage; retrieval scopes on `concept_tags`, not this. |
| `text` | string | **One atomic fact**, phrased as a self-contained statement. See "Why atomic" below. |
| `source.title` | string | The work the fact is drawn from (e.g. `OpenStax Biology 2e`, or `Synapse original`). |
| `source.section` | string | Chapter/section within the work, or `authored` for Synapse-original text. |
| `source.anchor` | string | A stable in-section anchor (subsection slug, figure id, or paragraph key) so the citation resolves to a specific place, not just a chapter. |
| `source.license` | string | SPDX-style license id. Allowed: `CC-BY-4.0` (OpenStax and other CC-BY sources) or `Synapse-Original` (text we wrote). Anything else fails validation. |

The `source` object becomes the user-visible citation rendered into the
`Grounding` field on the generated note (`M2 design §4.4`), and is copied into the
lineage row so a shipped note can always be traced back to its chunk.

## Why atomic

Each record is **one fact** for two reasons that compound:

1. **Retrieval precision.** Concept-scoped retrieval filters by `concept_tags`
   first, then vector-ranks within the concept (`M2 design §4.2`). Atomic chunks
   make the ranked grounding tight and on-topic; a multi-fact paragraph would
   pull in off-target claims and dilute the grounding the generator is
   constrained to.
2. **One-atomic-fact-per-card.** The product goal is atomic cards
   (`notes/prd.md` C1/B1). If the grounding is already atomic, a generated item
   maps cleanly to a single chunk, its citation is unambiguous, and the flaw
   checker's "single clear claim" rule is easy to satisfy.

If a source passage carries several facts, split it into several records with
distinct `id`s. Do **not** pack a table or a multi-clause explanation into one
`text`.

## Concept-scoped retrieval (how these records are used)

At generation time the service is asked to produce an item *for a specific
concept*. Retrieval then:

1. **Filters** `corpus_chunks` to rows whose `concept_tags` contain the target
   tag (a structured predicate — cheap and exact).
2. **Vector-ranks** the survivors against the query embedding and returns the
   top-k as grounding.

A generation request for a concept with **no** matching chunks returns no
grounding, and the generate pipeline **refuses** rather than inventing (the hard
grounding gate). That is why the seed set below covers exactly the four concepts
the client already ships.

## Adding records

1. Draw the fact from an openly-licensed source (OpenStax) or write it yourself.
2. Paraphrase into one atomic, correct, MCAT-level statement.
3. Tag it with the matching `concept::<section>::<id>` (create the concept on the
   client side first if it is new).
4. Fill real `source` metadata and the correct `license`.
5. Run `scripts/validate_corpus` (do not commit records that fail).
6. Re-run `scripts/ingest` to chunk, embed, and upsert (once keys exist).
