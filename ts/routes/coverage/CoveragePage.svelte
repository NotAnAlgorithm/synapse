<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<!--
=============================================================================
Synapse AAMC coverage checker (PRD B4).

Maps the user's cards against a hand-made AAMC-style content outline and shows,
per section/category, how many expected concepts they have a card for — and
leads with a "topics you haven't studied" list (the outline concepts with no
card yet), the actionable output of the check.

Pure presentation over the `conceptCoverage` RPC; no other backend calls.
=============================================================================
-->
<script lang="ts">
    import type { ConceptCoverageResponse } from "@generated/anki/stats_pb";
    import { localizedNumber } from "@tslib/i18n";

    import Container from "$lib/components/Container.svelte";
    import TitledContainer from "$lib/components/TitledContainer.svelte";

    import WithConceptCoverage from "./WithConceptCoverage.svelte";
    import { categoriesBySection, coveragePct, gaps, sectionLabel } from "./coverage";

    interface Props {
        /** Scope filter passed to the backend (e.g. "deck:Synapse").
         * Overridable so this page can be embedded against other scopes. */
        initialSearch?: string;
    }

    const { initialSearch = "deck:Synapse" }: Props = $props();

    // Scope tracks the prop for now (defaults to the Synapse deck). When a
    // search input is added, promote this to `$state` and bind it.
    const search = $derived(initialSearch);

    function categories(sourceData: ConceptCoverageResponse | null) {
        return sourceData?.categories ?? [];
    }
</script>

<Container class="coverage-page">
    <WithConceptCoverage {search}>
        {#snippet children({ sourceData, loading, error })}
            {@const cats = categories(sourceData)}
            {@const grouped = categoriesBySection(cats)}
            {@const gapList = gaps(cats)}

            {#if error}
                <TitledContainer title="Coverage">
                    <p class="muted">
                        Couldn't load your coverage data. Please try again.
                    </p>
                </TitledContainer>
            {:else if loading && cats.length === 0}
                <TitledContainer title="Coverage">
                    <p class="muted">Loading…</p>
                </TitledContainer>
            {:else if cats.length === 0}
                <TitledContainer title="Coverage">
                    <p class="muted">No outline available.</p>
                </TitledContainer>
            {:else}
                <!-- Hero: overall coverage against the outline -->
                <TitledContainer title="Outline coverage">
                    <div class="hero">
                        <div class="hero-figure">
                            {coveragePct(sourceData?.coverage ?? 0)}
                        </div>
                        <div class="hero-detail">
                            <div class="hero-band">
                                {localizedNumber(sourceData?.coveredCount ?? 0, 0)} of {localizedNumber(
                                    sourceData?.expectedCount ?? 0,
                                    0,
                                )} outline topics have a card
                            </div>
                            <p class="muted">
                                How much of the seed AAMC content outline your cards
                                touch. A topic counts as covered once you have at least
                                one card tagged to it.
                            </p>
                        </div>
                    </div>
                </TitledContainer>

                <!-- The actionable output: topics you haven't studied (gaps) -->
                <TitledContainer title="Topics you haven't studied">
                    {#if gapList.length === 0}
                        <p class="muted">
                            No gaps — you have at least one card for every topic in the
                            outline. Nice.
                        </p>
                    {:else}
                        <p class="muted small">
                            Outline topics with no card yet — the blind spots to make a
                            card for next.
                        </p>
                        <ul class="gaps">
                            {#each gapList as gap (gap.concept)}
                                <li>
                                    <span class="gap-name">{gap.name}</span>
                                    <span class="gap-loc muted small">
                                        {sectionLabel(gap.section)} · {gap.categoryName}
                                    </span>
                                </li>
                            {/each}
                        </ul>
                    {/if}
                </TitledContainer>

                <!-- Per-section / per-category breakdown -->
                {#each grouped as group (group.section)}
                    <TitledContainer title={sectionLabel(group.section)}>
                        <div class="cat-grid">
                            {#each group.categories as category (category.id)}
                                <div class="cat-card">
                                    <div class="cat-head">
                                        <span class="cat-name">{category.name}</span>
                                        <span class="cat-pct">
                                            {coveragePct(category.coverage)}
                                        </span>
                                    </div>
                                    <div class="cat-count muted small">
                                        {localizedNumber(
                                            category.coveredCount,
                                            0,
                                        )}/{localizedNumber(category.expectedCount, 0)} concepts
                                    </div>
                                    <ul class="concepts">
                                        {#each category.concepts as concept (concept.concept)}
                                            <li class:covered={concept.covered}>
                                                <span
                                                    class="dot"
                                                    aria-hidden="true"
                                                ></span>
                                                <span class="concept-name">
                                                    {concept.name}
                                                </span>
                                                {#if !concept.covered}
                                                    <span class="chip">no card</span>
                                                {/if}
                                            </li>
                                        {/each}
                                    </ul>
                                </div>
                            {/each}
                        </div>
                    </TitledContainer>
                {/each}
            {/if}
        {/snippet}
    </WithConceptCoverage>
    <div class="spacer"></div>
</Container>

<style lang="scss">
    :global(.coverage-page) {
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

    // Hero -----------------------------------------------------------------
    .hero {
        display: flex;
        align-items: center;
        gap: 1.5rem;
        padding: 0.5rem 0;
    }

    .hero-figure {
        font-size: 3.5rem;
        font-weight: 700;
        line-height: 1;
        min-width: 4ch;
        font-variant-numeric: tabular-nums;
    }

    .hero-band {
        font-size: 1.15rem;
        font-weight: 600;
        margin-bottom: 0.25rem;
    }

    .hero-detail p {
        margin: 0.15rem 0;
    }

    // Gaps -----------------------------------------------------------------
    .gaps {
        list-style: none;
        margin: 0.5rem 0 0;
        padding: 0;
    }

    .gaps li {
        display: grid;
        grid-template-columns: 1fr auto;
        align-items: baseline;
        gap: 0.75rem;
        padding: 0.4rem 0;
        border-bottom: 1px solid var(--border-subtle);
    }

    .gaps li:last-child {
        border-bottom: none;
    }

    .gap-name {
        font-weight: 500;
    }

    // Category grid --------------------------------------------------------
    .cat-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
        gap: 0.75rem;
        margin-top: 0.5rem;
    }

    .cat-card {
        border: 1px solid var(--border-subtle);
        border-radius: var(--border-radius-medium, 10px);
        padding: 0.75rem;
        background: var(--canvas);
    }

    .cat-head {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
        gap: 0.5rem;
    }

    .cat-name {
        font-weight: 600;
    }

    .cat-pct {
        font-weight: 700;
        font-variant-numeric: tabular-nums;
    }

    .concepts {
        list-style: none;
        margin: 0.5rem 0 0;
        padding: 0;
    }

    .concepts li {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        padding: 0.15rem 0;
        color: var(--fg-subtle);
    }

    .concepts li.covered {
        color: var(--fg);
    }

    .dot {
        width: 0.55em;
        height: 0.55em;
        border-radius: 50%;
        flex: none;
        border: 1px solid var(--border-subtle);
        background: transparent;
    }

    .concepts li.covered .dot {
        background: var(--accent, #2e7d32);
        border-color: var(--accent, #2e7d32);
    }

    .concept-name {
        flex: 1;
    }

    .chip {
        display: inline-block;
        font-size: 0.75em;
        padding: 0.05rem 0.45rem;
        border-radius: 999px;
        border: 1px solid var(--border-subtle);
        color: var(--fg-subtle);
    }

    .spacer {
        height: 1.5em;
    }
</style>
