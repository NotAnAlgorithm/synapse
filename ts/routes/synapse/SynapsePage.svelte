<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<!--
=============================================================================
Synapse M0 "Memory" dashboard.

Leads with LEARNING, not activity (PRD E1): the hero is an Overall Memory
figure (mean FSRS retrievability, weighted by scored cards) plus a
plain-language "how sure" hint. Per-section rollups and a per-concept table
give a breakdown (PRD F1), and a "Work on next" list surfaces the weakest
concepts as the highest-leverage things to study (PRD F1). When too few
concepts have enough data we abstain and show a "keep practicing" state
instead of a number (PRD F4).

Card counts / streaks are intentionally de-emphasised to coverage-only
secondary metrics — that de-emphasis is the whole point of E1.

Pure presentation over the `conceptMemory` RPC; no other backend calls.
=============================================================================
-->
<script lang="ts">
    import type { ConceptMemoryResponse } from "@generated/anki/stats_pb";
    import { localizedNumber } from "@tslib/i18n";

    import Container from "$lib/components/Container.svelte";
    import TitledContainer from "$lib/components/TitledContainer.svelte";

    import AdoptionSection from "./AdoptionSection.svelte";
    import PerformanceSection from "./PerformanceSection.svelte";
    import WithConceptMemory from "./WithConceptMemory.svelte";
    import {
        conceptLabel,
        confidenceHint,
        memoryBand,
        overallMemory,
        sectionLabel,
        sectionRollups,
        workOnNext,
    } from "./memory";

    interface Props {
        /** Scope filter passed to the backend (e.g. "deck:MCAT").
         * Overridable so this page can be embedded against other scopes. */
        initialSearch?: string;
    }

    const { initialSearch = "deck:MCAT" }: Props = $props();

    // For M0 the scope tracks the prop (defaults to the Synapse deck). When a
    // search input is added, promote this to `$state` and bind it.
    const search = $derived(initialSearch);

    function memoryPct(value: number): string {
        return `${localizedNumber(value, 0)}%`;
    }

    function concepts(sourceData: ConceptMemoryResponse | null) {
        return sourceData?.concepts ?? [];
    }
</script>

<Container class="synapse-page">
    <WithConceptMemory {search}>
        {#snippet children({ sourceData, loading, error })}
            {@const rows = concepts(sourceData)}
            {@const overall = overallMemory(rows)}
            {@const sections = sectionRollups(rows)}
            {@const weakest = workOnNext(rows)}

            {#if error}
                <TitledContainer title="Memory">
                    <p class="muted">
                        Couldn't load your Memory data. Please try again.
                    </p>
                </TitledContainer>
            {:else if loading && rows.length === 0}
                <TitledContainer title="Memory">
                    <p class="muted">Loading…</p>
                </TitledContainer>
            {:else if rows.length === 0}
                <TitledContainer title="Memory">
                    <p class="muted">
                        No concepts found yet. Tag your cards with
                        <code>concept::…</code>
                        tags and keep studying to build your Memory picture.
                    </p>
                </TitledContainer>
            {:else}
                <!-- Hero: Overall Memory (E1) -->
                <TitledContainer title="Overall Memory">
                    {#if overall.memory === null}
                        <div class="hero abstain">
                            <div class="hero-figure">—</div>
                            <div class="hero-detail">
                                <div class="hero-band">Keep practicing</div>
                                <p class="muted">
                                    Not enough data yet. Once a few more concepts have
                                    some review history, we'll show your overall Memory
                                    here.
                                </p>
                                <p class="muted small">
                                    {overall.sufficientConcepts} of {overall.totalConcepts}
                                    concepts have enough data so far.
                                </p>
                            </div>
                        </div>
                    {:else}
                        <div class="hero">
                            <div class="hero-figure">
                                {memoryPct(overall.memory)}
                            </div>
                            <div class="hero-detail">
                                <div class="hero-band">
                                    {memoryBand(overall.memory)}
                                </div>
                                <p class="muted">
                                    How likely you are to recall a typical concept right
                                    now.
                                </p>
                                <p class="muted small">
                                    {confidenceHint(overall.scoredCardCount)}
                                </p>
                            </div>
                        </div>
                    {/if}
                </TitledContainer>

                <!-- Performance: provisional "can you apply it" score (F2).
                     Sits alongside Memory as one of the dashboard's lead scores
                     (E1); clearly labelled preliminary/uncalibrated. Self-shows
                     an honest empty state when there's no application history. -->
                <PerformanceSection {search} />

                <!-- Adoption: effort points + streak (E2/E3). Self-hides when
                     the default-off backend flag is disabled. -->
                <AdoptionSection {search} />

                <!-- Work on next: weakest concepts (F1) -->
                {#if weakest.length > 0}
                    <TitledContainer title="Work on next">
                        <p class="muted small">
                            Your weakest concepts right now — the biggest wins if you
                            study them.
                        </p>
                        <ul class="worklist">
                            {#each weakest as concept (concept.concept)}
                                <li>
                                    <span class="worklist-name">
                                        {conceptLabel(concept.concept)}
                                    </span>
                                    <span class="worklist-section muted small">
                                        {sectionLabel(concept.section)}
                                    </span>
                                    <span
                                        class="worklist-memory"
                                        class:weak={concept.memory < 60}
                                    >
                                        {memoryPct(concept.memory)}
                                    </span>
                                </li>
                            {/each}
                        </ul>
                    </TitledContainer>
                {/if}

                <!-- Per-section rollups (F1) -->
                {#if sections.length > 0}
                    <TitledContainer title="By section">
                        <div class="section-grid">
                            {#each sections as section (section.section)}
                                <div class="section-card">
                                    <div class="section-name">
                                        {sectionLabel(section.section)}
                                    </div>
                                    {#if section.memory === null}
                                        <div class="section-memory muted">
                                            Needs data
                                        </div>
                                    {:else}
                                        <div class="section-memory">
                                            {memoryPct(section.memory)}
                                        </div>
                                    {/if}
                                    <div class="section-coverage muted small">
                                        {section.sufficientConcepts}/{section.totalConcepts}
                                        concepts
                                    </div>
                                </div>
                            {/each}
                        </div>
                    </TitledContainer>
                {/if}

                <!-- Per-concept table (F1) -->
                <TitledContainer title="All concepts">
                    <div class="table-wrap">
                        <table class="concepts">
                            <thead>
                                <tr>
                                    <th>Concept</th>
                                    <th>Section</th>
                                    <th class="num">Memory</th>
                                    <th class="num">Cards</th>
                                </tr>
                            </thead>
                            <tbody>
                                {#each rows as concept (concept.concept)}
                                    <tr class:muted-row={!concept.sufficientData}>
                                        <td>{conceptLabel(concept.concept)}</td>
                                        <td class="muted small">
                                            {sectionLabel(concept.section)}
                                        </td>
                                        <td class="num">
                                            {#if concept.sufficientData}
                                                {memoryPct(concept.memory)}
                                            {:else}
                                                <span class="chip">needs data</span>
                                            {/if}
                                        </td>
                                        <td class="num muted small">
                                            {localizedNumber(concept.cardCount, 0)}
                                        </td>
                                    </tr>
                                {/each}
                            </tbody>
                        </table>
                    </div>
                </TitledContainer>
            {/if}
        {/snippet}
    </WithConceptMemory>
    <div class="spacer"></div>
</Container>

<style lang="scss">
    :global(.synapse-page) {
        gap: 1em;
        padding: 1em;
        max-width: 1100px;
    }

    .muted {
        color: var(--fg-subtle);
    }

    .small {
        font-size: 0.85em;
    }

    code {
        background: var(--canvas);
        border-radius: var(--border-radius, 5px);
        padding: 0 0.25em;
    }

    // Hero -----------------------------------------------------------------
    .hero {
        display: flex;
        align-items: center;
        gap: 1.5rem;
        padding: 0.5rem 0;

        &.abstain .hero-figure {
            color: var(--fg-subtle);
        }
    }

    .hero-figure {
        font-size: 3.5rem;
        font-weight: 700;
        line-height: 1;
        min-width: 4ch;
    }

    .hero-band {
        font-size: 1.15rem;
        font-weight: 600;
        margin-bottom: 0.25rem;
    }

    .hero-detail p {
        margin: 0.15rem 0;
    }

    // Work on next ---------------------------------------------------------
    .worklist {
        list-style: none;
        margin: 0.5rem 0 0;
        padding: 0;
    }

    .worklist li {
        display: grid;
        grid-template-columns: 1fr auto auto;
        align-items: baseline;
        gap: 0.75rem;
        padding: 0.4rem 0;
        border-bottom: 1px solid var(--border-subtle);
    }

    .worklist li:last-child {
        border-bottom: none;
    }

    .worklist-name {
        font-weight: 500;
    }

    .worklist-memory {
        font-variant-numeric: tabular-nums;
        font-weight: 600;

        &.weak {
            color: var(--fg-error, #c0392b);
        }
    }

    // Sections -------------------------------------------------------------
    .section-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
        gap: 0.75rem;
        margin-top: 0.5rem;
    }

    .section-card {
        border: 1px solid var(--border-subtle);
        border-radius: var(--border-radius-medium, 10px);
        padding: 0.75rem;
        background: var(--canvas);
    }

    .section-name {
        font-weight: 600;
        text-transform: capitalize;
    }

    .section-memory {
        font-size: 1.75rem;
        font-weight: 700;
        margin: 0.2rem 0;
        font-variant-numeric: tabular-nums;

        &.muted {
            font-size: 1rem;
            font-weight: 500;
        }
    }

    // Table ----------------------------------------------------------------
    .table-wrap {
        overflow-x: auto;
        margin-top: 0.5rem;
    }

    table.concepts {
        width: 100%;
        border-collapse: collapse;
    }

    table.concepts th,
    table.concepts td {
        text-align: left;
        padding: 0.4rem 0.6rem;
        border-bottom: 1px solid var(--border-subtle);
    }

    table.concepts th.num,
    table.concepts td.num {
        text-align: right;
        font-variant-numeric: tabular-nums;
    }

    table.concepts tr.muted-row td {
        color: var(--fg-subtle);
    }

    .section-name,
    td {
        text-transform: capitalize;
    }

    .chip {
        display: inline-block;
        font-size: 0.75em;
        padding: 0.05rem 0.45rem;
        border-radius: 999px;
        border: 1px solid var(--border-subtle);
        color: var(--fg-subtle);
        text-transform: none;
    }

    .spacer {
        height: 1.5em;
    }
</style>
