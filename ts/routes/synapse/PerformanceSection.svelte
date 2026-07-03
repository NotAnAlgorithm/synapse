<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<!--
=============================================================================
Synapse PROVISIONAL Performance section (PRD F2).

Where Memory (F1) asks "can you RECALL this concept", Performance asks the
harder question "can you APPLY it to a novel, exam-style item". It is driven by
answer history on APPLICATION-form cards, blended with current retrievability
and prerequisite mastery.

IMPORTANT — this score is PRELIMINARY / UNCALIBRATED. There is no held-out AAMC
data to calibrate against yet (PRD F2 wants a calibrated probability before the
number can be trusted), so we show a transparent provisional blend and label it
loudly as preliminary. Thin data (fewer than a few application attempts) is
flagged rather than dressed up (F4 abstention spirit) — but per the owner
decision the provisional number is still shown.

Pure presentation over the `conceptPerformance` RPC.
=============================================================================
-->
<script lang="ts">
    import type { ConceptPerformanceResponse } from "@generated/anki/stats_pb";
    import { localizedNumber } from "@tslib/i18n";

    import TitledContainer from "$lib/components/TitledContainer.svelte";

    import { conceptLabel, sectionLabel } from "./memory";
    import {
        hasApplicationData,
        performanceDrivers,
        weakestApplications,
    } from "./performance";
    import WithConceptPerformance from "./WithConceptPerformance.svelte";

    interface Props {
        /** Scope filter passed to the backend (e.g. "deck:Synapse"). */
        search: string;
    }

    const { search }: Props = $props();

    function performancePct(value: number): string {
        return `${localizedNumber(value, 0)}%`;
    }

    function concepts(sourceData: ConceptPerformanceResponse | null) {
        return sourceData?.concepts ?? [];
    }
</script>

<WithConceptPerformance {search}>
    {#snippet children({ sourceData, loading, error })}
        {@const rows = concepts(sourceData)}
        {@const weakest = weakestApplications(rows)}

        {#if error}
            <TitledContainer title="Performance">
                <p class="muted">
                    Couldn't load your Performance data. Please try again.
                </p>
            </TitledContainer>
        {:else if loading && rows.length === 0}
            <TitledContainer title="Performance">
                <p class="muted">Loading…</p>
            </TitledContainer>
        {:else if !hasApplicationData(rows)}
            <TitledContainer title="Performance">
                <span class="prelim-tag">Preliminary — uncalibrated</span>
                <p class="muted">
                    No application practice yet. Performance measures whether you can <em
                    >
                        apply
                    </em>
                    a concept to a new, exam-style question — not just recall it. Study some
                    application cards (the "MCAT Application" style) to start building this
                    picture.
                </p>
            </TitledContainer>
        {:else}
            <TitledContainer title="Performance">
                <!-- The whole section is provisional: say so, prominently and
                     first, so no one reads these as calibrated numbers. -->
                <div class="prelim-banner">
                    <span class="prelim-tag">Preliminary — uncalibrated</span>
                    <p class="muted small">
                        An early estimate of whether you can <em>apply</em>
                        each concept to a new question, from your application-item history.
                        It isn't calibrated against real exam data yet, so treat it as a rough,
                        directional signal — and expect a wider margin of error than your
                        Memory score.
                    </p>
                </div>

                <!-- Practice applying: weakest application concepts (F2/F1). -->
                {#if weakest.length > 0}
                    <div class="worklist-block">
                        <div class="block-title">Practice applying</div>
                        <p class="muted small">
                            Concepts you can recall but haven't shown you can apply yet
                            — the biggest transfer gaps.
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
                                        class="worklist-perf"
                                        class:weak={concept.performance < 50}
                                    >
                                        {performancePct(concept.performance)}
                                    </span>
                                </li>
                            {/each}
                        </ul>
                    </div>
                {/if}

                <!-- Per-concept table with drivers (F4 display contract). -->
                <div class="table-wrap">
                    <table class="concepts">
                        <thead>
                            <tr>
                                <th>Concept</th>
                                <th>Section</th>
                                <th class="num">Performance</th>
                                <th class="num">Items</th>
                                <th>Why</th>
                            </tr>
                        </thead>
                        <tbody>
                            {#each rows as concept (concept.concept)}
                                {#if concept.appliedCount > 0}
                                    <tr class:muted-row={!concept.sufficientData}>
                                        <td>{conceptLabel(concept.concept)}</td>
                                        <td class="muted small">
                                            {sectionLabel(concept.section)}
                                        </td>
                                        <td class="num">
                                            <span
                                                class="perf-figure"
                                                class:weak={concept.performance < 50}
                                            >
                                                {performancePct(concept.performance)}
                                            </span>
                                            {#if !concept.sufficientData}
                                                <span class="chip">thin data</span>
                                            {/if}
                                        </td>
                                        <td class="num muted small">
                                            {localizedNumber(concept.appliedCount, 0)}
                                        </td>
                                        <td class="drivers muted small">
                                            {performanceDrivers(concept).join("; ")}
                                        </td>
                                    </tr>
                                {/if}
                            {/each}
                        </tbody>
                    </table>
                </div>
            </TitledContainer>
        {/if}
    {/snippet}
</WithConceptPerformance>

<style lang="scss">
    .muted {
        color: var(--fg-subtle);
    }

    .small {
        font-size: 0.85em;
    }

    em {
        font-style: italic;
    }

    // Preliminary label ----------------------------------------------------
    .prelim-tag {
        display: inline-block;
        font-size: 0.75em;
        font-weight: 600;
        letter-spacing: 0.02em;
        text-transform: uppercase;
        padding: 0.1rem 0.5rem;
        border-radius: 999px;
        border: 1px solid var(--border-subtle);
        color: var(--fg-subtle);
        background: var(--canvas);
    }

    .prelim-banner {
        padding: 0.25rem 0 0.5rem;

        p {
            margin: 0.4rem 0 0;
        }
    }

    // Practice applying ----------------------------------------------------
    .worklist-block {
        margin-top: 0.5rem;
        border-top: 1px solid var(--border-subtle);
        padding-top: 0.75rem;
    }

    .block-title {
        font-weight: 600;
        margin-bottom: 0.15rem;
    }

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
        text-transform: capitalize;
    }

    .worklist-section {
        text-transform: capitalize;
    }

    .worklist-perf {
        font-variant-numeric: tabular-nums;
        font-weight: 600;

        &.weak {
            color: var(--fg-error, #c0392b);
        }
    }

    // Table ----------------------------------------------------------------
    .table-wrap {
        overflow-x: auto;
        margin-top: 0.75rem;
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

    td {
        text-transform: capitalize;
    }

    .drivers {
        text-transform: none;
        max-width: 22rem;
    }

    .perf-figure {
        font-weight: 600;

        &.weak {
            color: var(--fg-error, #c0392b);
        }
    }

    .chip {
        display: inline-block;
        margin-left: 0.35rem;
        font-size: 0.75em;
        padding: 0.05rem 0.45rem;
        border-radius: 999px;
        border: 1px solid var(--border-subtle);
        color: var(--fg-subtle);
        text-transform: none;
    }
</style>
