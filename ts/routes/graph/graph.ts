// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//
// Pure helpers for the Synapse concept-graph visualization (PRD D1 / W3).
//
// Takes the ConceptGraphResponse (nodes = concepts with a Memory mastery
// signal, edges = directed prerequisite links) returned by the `conceptGraph`
// backend RPC and shapes it into the datstructures the d3 force layout drives,
// plus the mastery colour scale and label helpers. Kept free of Svelte/DOM so
// the shaping/colour logic is easy to test and reason about.
//
// Colour: mastery runs on a red→yellow→green scale (reusing d3's
// interpolateRdYlGn, the same scale the retrievability graph uses), with an
// explicit grey "abstain" colour for concepts without enough data (PRD F4) so
// a thin node is never shown a misleading colour.
//

import type {
    ConceptGraphResponse,
    ConceptGraphResponse_Edge as ProtoEdge,
    ConceptGraphResponse_Node as ProtoNode,
} from "@generated/anki/stats_pb";
import { interpolateRdYlGn } from "d3";
import type { SimulationLinkDatum, SimulationNodeDatum } from "d3";

/** Grey used for nodes without enough data to trust their Memory (F4). */
export const ABSTAIN_COLOUR = "#9aa0a6";

/** A concept node, extended with the mutable position fields d3-force writes. */
export interface GraphNode extends SimulationNodeDatum {
    /** full concept tag, e.g. concept::biochem::amino_acid_charge */
    id: string;
    /** 2nd tag segment, e.g. biochem ("" if absent) */
    section: string;
    /** mean retrievability 0..100 over scored cards */
    memory: number;
    /** total cards mapped to this concept (coverage) */
    cardCount: number;
    /** cards with memory_state contributing to `memory` */
    scoredCardCount: number;
    /** scored_card_count >= 3; when false the node is drawn grey (F4) */
    sufficientData: boolean;
}

/** A directed prerequisite link (`source` is a prerequisite of `target`). */
export type GraphLink = SimulationLinkDatum<GraphNode>;

export interface GraphData {
    nodes: GraphNode[];
    links: GraphLink[];
}

/**
 * Shape the proto response into the node/link arrays the force layout drives.
 *
 * Defensive against a malformed response: an edge whose endpoints aren't both
 * present as nodes is dropped (the backend already prunes these, but the layout
 * throws on a dangling link, so we guard again here).
 */
export function toGraphData(response: ConceptGraphResponse | null): GraphData {
    const protoNodes: ProtoNode[] = response?.nodes ?? [];
    const protoEdges: ProtoEdge[] = response?.edges ?? [];

    const nodes: GraphNode[] = protoNodes.map((n) => ({
        id: n.concept,
        section: n.section,
        memory: n.memory,
        cardCount: n.cardCount,
        scoredCardCount: n.scoredCardCount,
        sufficientData: n.sufficientData,
    }));

    const ids = new Set(nodes.map((n) => n.id));
    const links: GraphLink[] = [];
    for (const edge of protoEdges) {
        if (ids.has(edge.fromConcept) && ids.has(edge.toConcept)) {
            links.push({ source: edge.fromConcept, target: edge.toConcept });
        }
    }

    return { nodes, links };
}

/**
 * Fill colour for a node. Nodes without sufficient data are drawn grey (F4);
 * otherwise Memory (0..100) maps onto a red→yellow→green scale.
 */
export function nodeColour(node: Pick<GraphNode, "memory" | "sufficientData">): string {
    if (!node.sufficientData) {
        return ABSTAIN_COLOUR;
    }
    // interpolateRdYlGn takes 0..1: 0 = red (weak), 1 = green (strong).
    const t = Math.max(0, Math.min(1, node.memory / 100));
    return interpolateRdYlGn(t);
}

/**
 * Human-friendly concept label: strip the leading `concept::section::` segments
 * and show the leaf tag with underscores turned into spaces. Falls back
 * gracefully for tags that don't follow the convention. (Mirrors the Memory
 * dashboard's `conceptLabel`.)
 */
export function conceptLabel(concept: string): string {
    const segments = concept.split("::");
    const leaf = segments[segments.length - 1] ?? concept;
    const cleaned = leaf.replace(/_/g, " ").trim();
    return cleaned.length > 0 ? cleaned : concept;
}

/** Human-friendly section label (underscores → spaces; empty → fallback). */
export function sectionLabel(section: string): string {
    const cleaned = section.replace(/_/g, " ").trim();
    return cleaned.length > 0 ? cleaned : "Uncategorized";
}

/** Resolve a link endpoint (which d3-force mutates from an id to a node) to its
 * concept id/tag, whether it's still a raw id or already a hydrated node. */
export function endpointId(endpoint: string | number | GraphNode): string {
    if (typeof endpoint === "object") {
        return endpoint.id;
    }
    return String(endpoint);
}

/** The prerequisites of a concept (edges pointing INTO it): concepts to master
 * first. Returned as concept tags. */
export function prerequisitesOf(concept: string, links: readonly GraphLink[]): string[] {
    const out: string[] = [];
    for (const link of links) {
        if (endpointId(link.target) === concept) {
            out.push(endpointId(link.source));
        }
    }
    return out;
}

/** The dependents of a concept (edges pointing OUT of it): concepts that need
 * it first. Returned as concept tags. */
export function dependentsOf(concept: string, links: readonly GraphLink[]): string[] {
    const out: string[] = [];
    for (const link of links) {
        if (endpointId(link.source) === concept) {
            out.push(endpointId(link.target));
        }
    }
    return out;
}
