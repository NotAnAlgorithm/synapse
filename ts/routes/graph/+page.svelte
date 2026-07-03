<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<!--
Synapse concept-graph visualization route entry (PRD D1 / W3).
Mirrors routes/synapse/+page.svelte: a thin entry that mounts the page
container. See GraphPage.svelte for the visualization itself.
-->
<script lang="ts">
    import GraphPage from "./GraphPage.svelte";
</script>

<GraphPage initialSearch="deck:Synapse" />
