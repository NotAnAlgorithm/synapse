<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<!--
=============================================================================
Synapse adoption section (PRD E2/E3).

Adherence *without* corrupting the objective. Two ideas, framed to reward
honest effort over activity padding (E1 spirit):

  - Points weighted toward DIFFICULT successful retrievals and lapse
    recoveries — grinding cards you already know earns almost nothing.
  - A streak with freeze/forgiveness, so a single off day doesn't wipe
    momentum.

Pure presentation over the `adoptionStats` RPC. The whole feature is behind a
default-off backend flag ("synapse:adoption_enabled"); the parent decides
whether to mount this section.
=============================================================================
-->
<script lang="ts">
    import type { AdoptionStatsResponse } from "@generated/anki/stats_pb";
    import { localizedNumber } from "@tslib/i18n";

    import TitledContainer from "$lib/components/TitledContainer.svelte";

    import { hardWinLabel, pointsSummary, streakSummary } from "./adoption";
    import WithAdoptionStats from "./WithAdoptionStats.svelte";

    interface Props {
        /** Scope filter passed to the backend (e.g. "deck:Synapse"). */
        search: string;
    }

    const { search }: Props = $props();

    function points(value: number): string {
        return localizedNumber(Math.round(value), 0);
    }

    function winTime(unixSecs: number): string {
        return new Date(unixSecs * 1000).toLocaleDateString();
    }

    function hasActivity(data: AdoptionStatsResponse): boolean {
        return data.successfulReviews > 0 || data.streakDays > 0;
    }
</script>

<WithAdoptionStats {search}>
    {#snippet children({ sourceData, loading, error })}
        {#if sourceData !== null && !sourceData.enabled}
            <!-- Feature is behind the default-off "synapse:adoption_enabled"
                 flag: render nothing until it's turned on. -->
        {:else if error}
            <TitledContainer title="Effort">
                <p class="muted">Couldn't load your effort stats. Please try again.</p>
            </TitledContainer>
        {:else if loading && sourceData === null}
            <TitledContainer title="Effort">
                <p class="muted">Loading…</p>
            </TitledContainer>
        {:else if sourceData === null || !hasActivity(sourceData)}
            <TitledContainer title="Effort">
                <p class="muted">
                    Keep studying to build a streak and earn points for your hardest
                    recalls.
                </p>
            </TitledContainer>
        {:else}
            <TitledContainer title="Effort">
                <div class="stat-row">
                    <!-- Streak: the momentum figure, framed by forgiving copy. -->
                    <div class="stat">
                        <div class="stat-figure" class:live={sourceData.streakDays > 0}>
                            {localizedNumber(sourceData.streakDays, 0)}
                        </div>
                        <div class="stat-label">day streak</div>
                        <p class="muted small">
                            {streakSummary(
                                sourceData.streakDays,
                                sourceData.freezesRemaining,
                                sourceData.studiedToday,
                            )}
                        </p>
                    </div>

                    <!-- Points: difficulty-weighted, anti-padding. -->
                    <div class="stat">
                        <div class="stat-figure">{points(sourceData.points)}</div>
                        <div class="stat-label">effort points</div>
                        <p class="muted small">
                            {pointsSummary(
                                sourceData.points,
                                sourceData.successfulReviews,
                                sourceData.lapseRecoveries,
                            )}
                        </p>
                    </div>
                </div>

                <!-- Recognition: the hardest wins, celebrated individually. -->
                {#if sourceData.hardestWins.length > 0}
                    <div class="wins">
                        <div class="wins-title">Your hardest wins</div>
                        <ul class="winlist">
                            {#each sourceData.hardestWins as win (win.cardId + "-" + win.reviewedAt)}
                                <li>
                                    <span class="win-label">{hardWinLabel(win)}</span>
                                    <span class="win-date muted small">
                                        {winTime(Number(win.reviewedAt))}
                                    </span>
                                    <span class="win-points">
                                        +{points(win.points)}
                                    </span>
                                </li>
                            {/each}
                        </ul>
                    </div>
                {/if}
            </TitledContainer>
        {/if}
    {/snippet}
</WithAdoptionStats>

<style lang="scss">
    .muted {
        color: var(--fg-subtle);
    }

    .small {
        font-size: 0.85em;
    }

    .stat-row {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
        gap: 1.5rem;
        padding: 0.5rem 0;
    }

    .stat-figure {
        font-size: 2.75rem;
        font-weight: 700;
        line-height: 1;
        font-variant-numeric: tabular-nums;

        &.live {
            color: var(--fg-accent, #2f6fed);
        }
    }

    .stat-label {
        font-size: 0.95rem;
        font-weight: 600;
        margin-top: 0.2rem;
    }

    .stat p {
        margin: 0.35rem 0 0;
    }

    // Hardest wins ---------------------------------------------------------
    .wins {
        margin-top: 0.75rem;
        border-top: 1px solid var(--border-subtle);
        padding-top: 0.75rem;
    }

    .wins-title {
        font-weight: 600;
        margin-bottom: 0.25rem;
    }

    .winlist {
        list-style: none;
        margin: 0;
        padding: 0;
    }

    .winlist li {
        display: grid;
        grid-template-columns: 1fr auto auto;
        align-items: baseline;
        gap: 0.75rem;
        padding: 0.35rem 0;
        border-bottom: 1px solid var(--border-subtle);
    }

    .winlist li:last-child {
        border-bottom: none;
    }

    .win-label {
        font-weight: 500;
    }

    .win-points {
        font-variant-numeric: tabular-nums;
        font-weight: 600;
        color: var(--fg-accent, #2f6fed);
    }
</style>
