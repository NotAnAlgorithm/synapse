<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<!--
Data loader for the Synapse adoption section (PRD E2/E3). Mirrors
WithConceptMemory: calls a single backend RPC (`adoptionStats`), tracks
loading/error state, and exposes the result to a snippet. Re-runs whenever
`search` changes, keeping old data visible during a reload so the panel
doesn't flash empty.
-->
<script lang="ts">
    import type { AdoptionStatsResponse } from "@generated/anki/stats_pb";
    import { adoptionStats } from "@generated/backend";
    import type { Snippet } from "svelte";

    interface Props {
        search: string;
        children: Snippet<
            [
                {
                    sourceData: AdoptionStatsResponse | null;
                    loading: boolean;
                    error: unknown;
                },
            ]
        >;
    }

    const { search, children }: Props = $props();

    let sourceData: AdoptionStatsResponse | null = $state(null);
    let loading = $state(true);
    let error: unknown = $state(null);

    async function updateSourceData(search: string): Promise<void> {
        loading = true;
        error = null;
        try {
            sourceData = await adoptionStats({ search });
        } catch (err) {
            error = err;
        } finally {
            loading = false;
        }
    }

    $effect(() => {
        updateSourceData(search);
    });
</script>

{@render children({ sourceData, loading, error })}
