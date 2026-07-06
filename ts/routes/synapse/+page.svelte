<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<!--
Synapse M0 "Memory" dashboard route entry (PRD E1/F1/F4).
Mirrors routes/graphs/+page.svelte: thin entry that mounts the page
container. See SynapsePage.svelte for the dashboard itself.
-->
<script lang="ts">
    import SynapsePage from "./SynapsePage.svelte";
</script>

<SynapsePage initialSearch="deck:MCAT" />
