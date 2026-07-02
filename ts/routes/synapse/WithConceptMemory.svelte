<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<!--
Data loader for the Synapse Memory dashboard. Mirrors graphs/WithGraphData:
calls a single backend RPC (`conceptMemory`), tracks loading/error state, and
exposes the result to a snippet. Re-runs whenever `search` changes. Keeps the
old data visible while a new query loads so the dashboard doesn't flash empty.
-->
<script lang="ts">
    import type { ConceptMemoryResponse } from "@generated/anki/stats_pb";
    import { conceptMemory } from "@generated/backend";
    import type { Snippet } from "svelte";

    interface Props {
        search: string;
        children: Snippet<
            [
                {
                    sourceData: ConceptMemoryResponse | null;
                    loading: boolean;
                    error: unknown;
                },
            ]
        >;
    }

    const { search, children }: Props = $props();

    let sourceData: ConceptMemoryResponse | null = $state(null);
    let loading = $state(true);
    let error: unknown = $state(null);

    async function updateSourceData(search: string): Promise<void> {
        loading = true;
        error = null;
        try {
            sourceData = await conceptMemory({ search });
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
