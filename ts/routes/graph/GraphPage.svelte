<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<!--
=============================================================================
Synapse concept-graph visualization (PRD D1 / W3).

An interactive node-link diagram of the M2 prerequisite graph: concepts are
nodes coloured by their Memory mastery (red = weak → green = strong; grey when
there's not enough data to trust, per F4), and prerequisite links are directed
arrows pointing from a prerequisite to the concept that depends on it. Pan and
zoom to explore, drag a node to pull the layout around, and click a node to see
its Memory and its prerequisites/dependents.

Layout is a d3 force simulation. d3-zoom / d3-drag are attached imperatively to
the SVG; node/link positions are mirrored into Svelte `$state` on each tick so
the markup stays declarative. Built entirely with the bundled d3 — no external
libraries (strict CSP).
=============================================================================
-->
<script lang="ts">
    import type { ConceptGraphResponse } from "@generated/anki/stats_pb";
    import { conceptGraph } from "@generated/backend";
    import {
        forceCenter,
        forceCollide,
        forceLink,
        forceManyBody,
        forceSimulation,
    } from "d3";
    import type { Simulation } from "d3";
    import { drag as d3drag } from "d3";
    import type { SubjectPosition } from "d3";
    import { localizedNumber } from "@tslib/i18n";
    import { select } from "d3";
    import { zoom as d3zoom, zoomIdentity } from "d3";
    import type { ZoomTransform } from "d3";
    import { untrack } from "svelte";

    import Container from "$lib/components/Container.svelte";
    import TitledContainer from "$lib/components/TitledContainer.svelte";

    import {
        conceptLabel,
        dependentsOf,
        endpointId,
        type GraphLink,
        type GraphNode,
        nodeColour,
        prerequisitesOf,
        sectionLabel,
        toGraphData,
    } from "./graph";

    interface Props {
        /** Scope filter passed to the backend (e.g. "deck:MCAT").
         * Overridable so this page can be embedded against other scopes. */
        initialSearch?: string;
    }

    const { initialSearch = "deck:MCAT" }: Props = $props();

    // Scope tracks the prop for now (defaults to the MCAT deck). When a
    // search input is added, promote this to `$state` and bind it.
    const search = $derived(initialSearch);

    // Canvas size in CSS px. Tracked (not constant) so the layout fills — and
    // re-centres within — the actual available area, adapting to window/screen
    // resizes. Seeded with sensible defaults until the ResizeObserver below
    // measures the real element.
    let width = $state(900);
    let height = $state(620);
    const NODE_RADIUS = 11;
    // Keep the arrowhead clear of the target node's circle.
    const ARROW_GAP = NODE_RADIUS + 3;

    // Node radius scales gently with how many cards feed the concept, so a
    // well-populated concept reads as a bigger node without dominating.
    function radiusFor(node: GraphNode): number {
        return NODE_RADIUS + Math.min(6, Math.sqrt(node.cardCount));
    }

    // Position transform for a node. Takes the per-tick `frame` counter purely
    // so this read re-runs as d3 moves the node in place (`_frame` is unused).
    function nodeTransform(node: GraphNode, _frame: number): string {
        return `translate(${node.x ?? 0},${node.y ?? 0})`;
    }

    let svgEl: SVGSVGElement | null = $state(null);
    let wrapEl: HTMLDivElement | null = $state(null);

    // --- data loading --------------------------------------------------------
    // Loaded directly here (rather than via a data-loader wrapper) so the force
    // simulation can be (re)built from a proper `$effect`, never from render.
    let sourceData = $state<ConceptGraphResponse | null>(null);
    let loading = $state(true);
    let error = $state<unknown>(null);

    async function loadData(searchScope: string): Promise<void> {
        loading = true;
        error = null;
        try {
            sourceData = await conceptGraph({ search: searchScope });
        } catch (err) {
            error = err;
        } finally {
            loading = false;
        }
    }

    // Re-fetch whenever the scope changes.
    $effect(() => {
        loadData(search);
    });

    // Layout state. `nodes`/`links`/`transform` use `$state.raw` deliberately:
    // d3-force and d3-zoom OWN these objects and mutate `node.x/y/vx/vy` (and
    // the ZoomTransform) in place ~60×/s. A plain `$state` would deep-proxy
    // them, so every physics tick would fire a storm of per-property reactive
    // writes. With `.raw`, only our explicit reassignments (rebuilding the graph
    // in buildSimulation, and swapping in a fresh ZoomTransform on zoom) are
    // reactive.
    //
    // Per-tick redraws are driven by `frame` (below), NOT by reassigning the
    // arrays: d3 mutates the SAME node objects in place, so `nodes = [...nodes]`
    // is a no-op for a keyed {#each} — Svelte compares items by reference (its
    // default `equals`) and the instances are unchanged, so per-node transforms
    // would never re-run and the nodes would stay frozen at their initial
    // near-origin positions.
    let nodes = $state.raw<GraphNode[]>([]);
    let links = $state.raw<GraphLink[]>([]);
    let transform = $state.raw<ZoomTransform>(zoomIdentity);
    let selectedId = $state<string | null>(null);
    let hoverId = $state<string | null>(null);
    /** Bumped on every simulation tick to pulse the position-dependent reads
     * (node transforms + edge geometry) as d3 mutates coordinates in place. */
    let frame = $state(0);

    let simulation: Simulation<GraphNode, GraphLink> | null = null;
    /** The zoom behavior bound to the <svg>; kept so "Reset view" can drive it
     * (calling `.transform` on it updates the SVG's stored zoom state, so a
     * later manual pan/zoom continues smoothly instead of jumping). */
    let zoomBehavior: ReturnType<typeof d3zoom<SVGSVGElement, unknown>> | null = null;
    /** Bumped only when the NODE SET is rebuilt (not on every tick). The drag
     * effect keys off this so drag handlers are re-bound when nodes change, but
     * not on every animation frame. */
    let layoutVersion = $state(0);

    function stopSimulation(): void {
        simulation?.stop();
        simulation = null;
    }

    function buildSimulation(data: ConceptGraphResponse | null): void {
        stopSimulation();
        const graph = toGraphData(data);
        nodes = graph.nodes;
        links = graph.links;
        layoutVersion += 1;
        // Keep a still-present selection; otherwise clear it.
        selectedId =
            selectedId && nodes.some((n) => n.id === selectedId) ? selectedId : null;

        if (nodes.length === 0) {
            return;
        }

        simulation = forceSimulation<GraphNode, GraphLink>(nodes)
            .force(
                "link",
                forceLink<GraphNode, GraphLink>(links)
                    .id((n) => n.id)
                    .distance(72)
                    .strength(0.6),
            )
            // Gentler repulsion with a capped range keeps the graph a compact
            // cluster instead of flinging nodes to the edges of the (now larger,
            // adaptive) canvas.
            .force("charge", forceManyBody().strength(-220).distanceMax(320))
            .force("center", forceCenter(width / 2, height / 2))
            .force(
                "collide",
                forceCollide<GraphNode>().radius((n) => radiusFor(n) + 10),
            )
            .on("tick", () => {
                // d3 mutates node.x/y and the hydrated link endpoints in place;
                // pulse `frame` so the position-dependent reads (each node's
                // transform and the `drawn` edge geometry) re-run and mirror the
                // new coordinates into the DOM. Reassigning `nodes`/`links` here
                // would NOT update anything: the keyed {#each} compares items by
                // reference and these are the same instances.
                frame += 1;
            });
    }

    // Rebuild the simulation whenever the loaded data changes. `sourceData` is
    // the *only* intended dependency; the rebuild writes `nodes`/`links`/
    // `layoutVersion`/`selectedId` (and reads some of them), so it must run
    // untracked — otherwise the effect would depend on state it mutates and
    // re-trigger itself forever (Svelte `effect_update_depth_exceeded`).
    $effect(() => {
        const data = sourceData;
        untrack(() => buildSimulation(data));
        return () => stopSimulation();
    });

    // Attach zoom/pan + node drag once the <svg> is mounted.
    $effect(() => {
        if (!svgEl) {
            return;
        }
        const svg = select(svgEl);
        const behavior = d3zoom<SVGSVGElement, unknown>()
            .scaleExtent([0.2, 4])
            .on("zoom", (event) => {
                transform = event.transform;
            });
        zoomBehavior = behavior;
        svg.call(behavior);

        return () => {
            svg.on(".zoom", null);
            zoomBehavior = null;
        };
    });

    // Track the rendered canvas size so the SVG's coordinate space matches its
    // pixel size 1:1 and the layout uses the real available area. Re-measures on
    // any element/window resize.
    $effect(() => {
        if (!wrapEl) {
            return;
        }
        const observer = new ResizeObserver((entries) => {
            const rect = entries[0]?.contentRect;
            if (rect && rect.width > 0 && rect.height > 0) {
                width = Math.round(rect.width);
                height = Math.round(rect.height);
            }
        });
        observer.observe(wrapEl);
        return () => observer.disconnect();
    });

    // Keep the centring force aligned with the current canvas size and nudge the
    // simulation so the graph glides back to centre after a resize. Runs on size
    // changes; a freshly built simulation is already centred by buildSimulation.
    $effect(() => {
        const cx = width / 2;
        const cy = height / 2;
        if (!simulation) {
            return;
        }
        simulation.force("center", forceCenter(cx, cy));
        simulation.alpha(0.3).restart();
    });

    // Node dragging: (re)bound whenever the node set changes (via
    // `layoutVersion`), NOT on every tick. d3-drag is attached to the rendered
    // `g.node` elements.
    $effect(() => {
        // re-run when the node set is rebuilt (not on per-tick position updates)
        void layoutVersion;
        if (!svgEl || !simulation) {
            return;
        }
        const sim = simulation;
        const svgNode = svgEl;
        const dragBehavior = d3drag<SVGGElement, GraphNode, SubjectPosition>()
            // Measure the pointer against the <svg> (screen space) rather than
            // the drag target's transformed parent, so undoing the zoom
            // transform below gives the correct simulation-space position at any
            // zoom level.
            .container(svgNode)
            // d3-drag reports `event.x/y` relative to the SUBJECT, not the raw
            // pointer. Report the node in the SAME <svg> (zoomed/panned) space
            // the pointer is measured in — its on-screen position — so the two
            // aren't mixed. Reporting the node's raw layout coords here (the
            // default subject) offsets the grab by the current pan/zoom, making
            // the node jump when you start dragging after panning.
            .subject((_event, d) => ({
                x: transform.applyX(d.x ?? 0),
                y: transform.applyY(d.y ?? 0),
            }))
            .on("start", (event, d) => {
                if (!event.active) {
                    sim.alphaTarget(0.3).restart();
                }
                d.fx = d.x;
                d.fy = d.y;
            })
            .on("drag", (event, d) => {
                // event.x/y are in <svg> space; undo the zoom/pan transform to
                // get the layout coordinate to pin the node at.
                d.fx = transform.invertX(event.x);
                d.fy = transform.invertY(event.y);
            })
            .on("end", (event, d) => {
                if (!event.active) {
                    sim.alphaTarget(0);
                }
                d.fx = null;
                d.fy = null;
            });
        // Bind each node's datum to its rendered <g.node> and attach the drag
        // behaviour. `.data(nodes)` joins BY INDEX (no key function): Svelte
        // owns these elements, so d3's `__data__` is unset on them — passing a
        // key accessor like `(d) => d.id` would call it with the existing DOM
        // data (`undefined`) and throw, which previously aborted this effect's
        // flush and froze the whole layout (nodes stuck at their initial
        // near-origin positions, and zoom/pan dead). Index order is correct:
        // the keyed {#each} renders `g.node` elements in `nodes` order, so
        // element i pairs with nodes[i]. Read `nodes` untracked — the node set
        // only changes on rebuild (via `layoutVersion`, our trigger above); the
        // per-tick `frame` pulse never reassigns it, so drag handlers bind once
        // per rebuild rather than ~60×/s.
        untrack(() =>
            select(svgNode)
                .selectAll<SVGGElement, GraphNode>("g.node")
                .data(nodes)
                .call(dragBehavior),
        );
    });

    function resetView(): void {
        if (!svgEl || !zoomBehavior) {
            return;
        }
        // Reset the bound zoom behavior's stored transform so subsequent
        // pan/zoom continues from the identity rather than jumping.
        select(svgEl)
            .transition()
            .duration(300)
            .call(zoomBehavior.transform, zoomIdentity);
        transform = zoomIdentity;
        simulation?.alpha(0.5).restart();
    }

    function onNodeClick(id: string): void {
        selectedId = selectedId === id ? null : id;
    }

    // --- selected-node detail ------------------------------------------------
    const selectedNode = $derived(
        selectedId ? (nodes.find((n) => n.id === selectedId) ?? null) : null,
    );
    const selectedPrereqs = $derived(
        selectedId ? prerequisitesOf(selectedId, links) : [],
    );
    const selectedDependents = $derived(
        selectedId ? dependentsOf(selectedId, links) : [],
    );

    function memoryPct(node: GraphNode): string {
        if (!node.sufficientData) {
            return "not enough data";
        }
        return `${localizedNumber(node.memory, 0)}%`;
    }

    // Edge endpoints, shortened so the arrowhead sits just outside the target.
    interface DrawnLink {
        key: string;
        x1: number;
        y1: number;
        x2: number;
        y2: number;
        active: boolean;
    }
    function drawnLinks(
        ls: readonly GraphLink[],
        highlight: string | null,
    ): DrawnLink[] {
        const out: DrawnLink[] = [];
        for (const link of ls) {
            const s = link.source as GraphNode;
            const t = link.target as GraphNode;
            if (
                typeof s !== "object" ||
                typeof t !== "object" ||
                s.x == null ||
                s.y == null ||
                t.x == null ||
                t.y == null
            ) {
                continue;
            }
            const dx = t.x - s.x;
            const dy = t.y - s.y;
            const dist = Math.hypot(dx, dy) || 1;
            const ux = dx / dist;
            const uy = dy / dist;
            const sr = radiusFor(s);
            out.push({
                key: `${s.id}>${t.id}`,
                x1: s.x + ux * sr,
                y1: s.y + uy * sr,
                x2: t.x - ux * ARROW_GAP,
                y2: t.y - uy * ARROW_GAP,
                active: highlight != null && (s.id === highlight || t.id === highlight),
            });
        }
        return out;
    }

    // The concept currently emphasised: hover takes priority over selection.
    const highlight = $derived(hoverId ?? selectedId);
    // Edge geometry for the current layout, recomputed as the sim ticks
    // (`frame`) and when the link set or highlight changes. `drawnLinks` reads
    // the live, d3-mutated endpoint coordinates, so it must re-run per tick.
    const drawn = $derived.by(() => {
        void frame;
        return drawnLinks(links, highlight);
    });

    function isDimmed(
        id: string,
        highlight: string | null,
        ls: readonly GraphLink[],
    ): boolean {
        if (highlight == null) {
            return false;
        }
        if (id === highlight) {
            return false;
        }
        for (const link of ls) {
            if (
                (endpointId(link.source) === highlight &&
                    endpointId(link.target) === id) ||
                (endpointId(link.target) === highlight &&
                    endpointId(link.source) === id)
            ) {
                return false;
            }
        }
        return true;
    }
</script>

<Container class="graph-page">
    {#if error}
        <TitledContainer title="Concept graph">
            <p class="muted">Couldn't load your concept graph. Please try again.</p>
        </TitledContainer>
    {:else if loading && nodes.length === 0}
        <TitledContainer title="Concept graph">
            <p class="muted">Loading…</p>
        </TitledContainer>
    {:else if nodes.length === 0}
        <TitledContainer title="Concept graph">
            <p class="muted">
                No concepts to graph yet. Tag your cards with
                <code>concept::…</code>
                tags and keep studying — concepts appear here as nodes, coloured by how well
                you know them, with prerequisite arrows between them.
            </p>
        </TitledContainer>
    {:else}
        <TitledContainer title="Concept graph" class="graph-card">
            <div class="toolbar">
                <p class="muted small">
                    Each node is a concept, coloured by Memory (red = weak, green =
                    strong, grey = not enough data). Arrows point from a prerequisite to
                    what depends on it. Scroll to zoom, drag the background to pan, drag
                    a node to move it, click a node for details.
                </p>
                <button class="reset" onclick={resetView}>Reset view</button>
            </div>

            <div class="canvas-wrap" bind:this={wrapEl}>
                <svg
                    bind:this={svgEl}
                    class="graph"
                    viewBox="0 0 {width} {height}"
                    role="img"
                    aria-label="Concept prerequisite graph"
                >
                    <defs>
                        <marker
                            id="synapse-arrow"
                            viewBox="0 0 10 10"
                            refX="9"
                            refY="5"
                            markerWidth="7"
                            markerHeight="7"
                            orient="auto-start-reverse"
                        >
                            <path d="M 0 0 L 10 5 L 0 10 z" class="arrowhead" />
                        </marker>
                        <marker
                            id="synapse-arrow-active"
                            viewBox="0 0 10 10"
                            refX="9"
                            refY="5"
                            markerWidth="7"
                            markerHeight="7"
                            orient="auto-start-reverse"
                        >
                            <path d="M 0 0 L 10 5 L 0 10 z" class="arrowhead-active" />
                        </marker>
                    </defs>

                    <g
                        transform="translate({transform.x},{transform.y}) scale({transform.k})"
                    >
                        <!-- edges -->
                        <g class="links">
                            {#each drawn as link (link.key)}
                                <line
                                    class="link"
                                    class:active={link.active}
                                    x1={link.x1}
                                    y1={link.y1}
                                    x2={link.x2}
                                    y2={link.y2}
                                    marker-end={link.active
                                        ? "url(#synapse-arrow-active)"
                                        : "url(#synapse-arrow)"}
                                />
                            {/each}
                        </g>

                        <!-- nodes -->
                        <g class="nodes">
                            {#each nodes as node (node.id)}
                                {@const dimmed = isDimmed(node.id, highlight, links)}
                                <g
                                    class="node"
                                    class:selected={selectedId === node.id}
                                    class:dimmed
                                    transform={nodeTransform(node, frame)}
                                    role="button"
                                    tabindex="0"
                                    aria-label={conceptLabel(node.id)}
                                    onclick={() => onNodeClick(node.id)}
                                    onkeydown={(e) => {
                                        if (e.key === "Enter" || e.key === " ") {
                                            e.preventDefault();
                                            onNodeClick(node.id);
                                        }
                                    }}
                                    onmouseenter={() => (hoverId = node.id)}
                                    onmouseleave={() => (hoverId = null)}
                                >
                                    <circle
                                        r={radiusFor(node)}
                                        fill={nodeColour(node)}
                                        class="node-circle"
                                    />
                                    <text class="node-label">
                                        {conceptLabel(node.id)}
                                    </text>
                                </g>
                            {/each}
                        </g>
                    </g>
                </svg>

                <!-- legend -->
                <div class="legend">
                    <div class="legend-title">Memory</div>
                    <div class="legend-scale">
                        <span class="swatch weak"></span>
                        <span class="swatch mid"></span>
                        <span class="swatch strong"></span>
                    </div>
                    <div class="legend-ends muted small">
                        <span>weak</span>
                        <span>strong</span>
                    </div>
                    <div class="legend-abstain muted small">
                        <span class="swatch abstain"></span>
                        not enough data
                    </div>
                </div>
            </div>
        </TitledContainer>

        <!-- selected node detail (F1 + prerequisites) -->
        {#if selectedNode}
            <TitledContainer title={conceptLabel(selectedNode.id)}>
                <div class="detail">
                    <div class="detail-row">
                        <span class="detail-key">Section</span>
                        <span class="detail-val">
                            {sectionLabel(selectedNode.section)}
                        </span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-key">Memory</span>
                        <span
                            class="detail-val"
                            class:muted={!selectedNode.sufficientData}
                        >
                            {memoryPct(selectedNode)}
                        </span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-key">Cards</span>
                        <span class="detail-val muted small">
                            {localizedNumber(selectedNode.cardCount, 0)}
                            ({localizedNumber(selectedNode.scoredCardCount, 0)}
                            scored)
                        </span>
                    </div>

                    <div class="detail-links">
                        <div class="detail-links-col">
                            <div class="detail-key">Prerequisites</div>
                            {#if selectedPrereqs.length === 0}
                                <p class="muted small">
                                    None — this is a foundational concept.
                                </p>
                            {:else}
                                <ul>
                                    {#each selectedPrereqs as pre (pre)}
                                        <li>
                                            <button
                                                class="linklike"
                                                onclick={() => (selectedId = pre)}
                                            >
                                                {conceptLabel(pre)}
                                            </button>
                                        </li>
                                    {/each}
                                </ul>
                            {/if}
                        </div>
                        <div class="detail-links-col">
                            <div class="detail-key">Unlocks</div>
                            {#if selectedDependents.length === 0}
                                <p class="muted small">Nothing depends on this yet.</p>
                            {:else}
                                <ul>
                                    {#each selectedDependents as dep (dep)}
                                        <li>
                                            <button
                                                class="linklike"
                                                onclick={() => (selectedId = dep)}
                                            >
                                                {conceptLabel(dep)}
                                            </button>
                                        </li>
                                    {/each}
                                </ul>
                            {/if}
                        </div>
                    </div>
                </div>
            </TitledContainer>
        {/if}
    {/if}
    <div class="spacer"></div>
</Container>

<style lang="scss">
    :global(.graph-page) {
        gap: 1em;
        padding: 1em;
        // Fill the whole window instead of a fixed-width column so the graph can
        // use the full dialog. `min-height` (not `height`) sidesteps the
        // Container's `.container-fluid { height: 100% }` rule; `box-sizing`
        // keeps the padding inside the viewport (no scrollbar).
        min-height: 100dvh;
        box-sizing: border-box;
    }

    // The graph card grows to fill the page; its content (toolbar + canvas) is a
    // flex column so the canvas takes all the remaining height.
    :global(.graph-card) {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
    }

    .muted {
        color: var(--fg-subtle);
    }

    .small {
        font-size: 0.92em;
    }

    code {
        background: var(--canvas);
        border-radius: var(--border-radius, 5px);
        padding: 0 0.25em;
    }

    .toolbar {
        display: flex;
        align-items: flex-start;
        gap: 1rem;
        justify-content: space-between;
        margin-bottom: 0.5rem;

        p {
            margin: 0;
            flex: 1;
        }
    }

    .reset {
        flex: none;
        padding: 0.35rem 0.75rem;
        border: 1px solid var(--border-subtle);
        border-radius: var(--border-radius, 5px);
        background: var(--canvas);
        color: var(--fg);
        cursor: pointer;

        &:hover {
            background: var(--canvas-elevated);
        }
    }

    .canvas-wrap {
        position: relative;
        width: 100%;
        // Take all the height the graph card leaves after the title + toolbar,
        // with a usable floor on very short windows (the page scrolls below it).
        // A window/screen resize changes this box and triggers a re-measure +
        // re-centre (ResizeObserver).
        flex: 1;
        min-height: 360px;
    }

    svg.graph {
        width: 100%;
        height: 100%;
        display: block;
        border: 1px solid var(--border-subtle);
        border-radius: var(--border-radius-medium, 10px);
        background: var(--canvas);
        cursor: grab;
        touch-action: none;

        &:active {
            cursor: grabbing;
        }
    }

    .link {
        stroke: var(--border-subtle);
        stroke-width: 1.5;

        &.active {
            stroke: var(--fg);
            stroke-width: 2.25;
        }
    }

    .arrowhead {
        fill: var(--border-subtle);
    }

    .arrowhead-active {
        fill: var(--fg);
    }

    .node {
        cursor: pointer;

        &.dimmed {
            opacity: 0.25;
        }
    }

    .node-circle {
        stroke: var(--canvas);
        stroke-width: 1.5;
    }

    .node.selected .node-circle {
        stroke: var(--fg);
        stroke-width: 2.5;
    }

    .node-label {
        font-size: 13px;
        fill: var(--fg);
        pointer-events: none;
        text-transform: capitalize;
        // Centre the label on the node (was offset to the right), so labels sit
        // symmetrically and overlap neighbours less.
        text-anchor: middle;
        dominant-baseline: central;
        paint-order: stroke;
        stroke: var(--canvas);
        stroke-width: 3px;
        stroke-linejoin: round;
    }

    // Legend ---------------------------------------------------------------
    .legend {
        position: absolute;
        top: 0.75rem;
        right: 0.75rem;
        background: var(--canvas-elevated);
        border: 1px solid var(--border-subtle);
        border-radius: var(--border-radius, 5px);
        padding: 0.5rem 0.65rem;
        font-size: 0.8rem;
    }

    .legend-title {
        font-weight: 600;
        margin-bottom: 0.3rem;
    }

    .legend-scale {
        display: flex;
        gap: 2px;
    }

    .legend-ends {
        display: flex;
        justify-content: space-between;
        margin-top: 0.15rem;
    }

    .legend-abstain {
        display: flex;
        align-items: center;
        gap: 0.35rem;
        margin-top: 0.4rem;
    }

    .swatch {
        display: inline-block;
        width: 1.4rem;
        height: 0.7rem;
        border-radius: 2px;
    }

    .legend-abstain .swatch {
        width: 0.85rem;
    }

    // The RdYlGn endpoints/midpoint, matching nodeColour().
    .swatch.weak {
        background: #d73027;
    }
    .swatch.mid {
        background: #ffffbf;
    }
    .swatch.strong {
        background: #1a9850;
    }
    .swatch.abstain {
        background: #9aa0a6;
    }

    // Detail panel ---------------------------------------------------------
    .detail-row {
        display: flex;
        gap: 0.75rem;
        padding: 0.25rem 0;
        align-items: baseline;
    }

    .detail-key {
        min-width: 7rem;
        font-weight: 600;
    }

    .detail-val {
        font-variant-numeric: tabular-nums;
        text-transform: capitalize;
    }

    .detail-links {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        gap: 1rem;
        margin-top: 0.75rem;
    }

    .detail-links ul {
        list-style: none;
        margin: 0.25rem 0 0;
        padding: 0;
    }

    .detail-links li {
        padding: 0.1rem 0;
    }

    .linklike {
        background: none;
        border: none;
        padding: 0;
        color: var(--fg-link, #2f6feb);
        cursor: pointer;
        font: inherit;
        text-align: left;
        text-transform: capitalize;

        &:hover {
            text-decoration: underline;
        }
    }

    .spacer {
        height: 1.5em;
    }
</style>
