<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<!--
Data loader for the Synapse AAMC coverage checker. Mirrors
synapse/WithConceptMemory: calls a single backend RPC (`conceptCoverage`),
tracks loading/error state, and exposes the result to a snippet. Re-runs
whenever `search` changes, keeping old data visible while a new query loads so
the page doesn't flash empty.

Note (integrator): `conceptCoverage` is generated from the ConceptCoverage RPC
appended to proto/anki/stats.proto — it exists in @generated/backend only after
a full codegen/build.
-->
<script lang="ts">
    import type { ConceptCoverageResponse } from "@generated/anki/stats_pb";
    import { conceptCoverage } from "@generated/backend";
    import type { Snippet } from "svelte";

    interface Props {
        search: string;
        children: Snippet<
            [
                {
                    sourceData: ConceptCoverageResponse | null;
                    loading: boolean;
                    error: unknown;
                },
            ]
        >;
    }

    const { search, children }: Props = $props();

    let sourceData: ConceptCoverageResponse | null = $state(null);
    let loading = $state(true);
    let error: unknown = $state(null);

    async function updateSourceData(search: string): Promise<void> {
        loading = true;
        error = null;
        try {
            sourceData = await conceptCoverage({ search });
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
