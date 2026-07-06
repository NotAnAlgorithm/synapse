<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<!--
Synapse AAMC coverage checker route entry (PRD B4).
Mirrors routes/synapse/+page.svelte: a thin entry that mounts the page
container. See CoveragePage.svelte for the checker itself.
-->
<script lang="ts">
    import CoveragePage from "./CoveragePage.svelte";
</script>

<CoveragePage initialSearch="deck:MCAT" />
