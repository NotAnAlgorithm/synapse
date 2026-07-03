# Synapse — State-Grounded AI Tutor (C2): Architecture & Design

**Status:** design proposal (M3) · Owner: e · Scope: DESIGN ONLY, no implementation
**Companion docs:** `notes/prd.md` (Feature C2, Principle 4, Principle 6, §11 non-goals), `notes/M2_service_layer_design.md` (the service-layer architecture this builds on — §5.1 already sketches how the tutor reads student state, and its appendix maps C2), roadmap `plans/velvet-brewing-cookie.md` (C2 is M3 / PRD Phase 3).

> This document designs the **state-grounded AI tutor** (`PRD C2`): a Socratic helper that appears at the moment of a miss, is grounded in the student's per-concept mastery map *and* the verified answer for the item, surfaces the unmastered *prerequisite* behind the mistake, and is guardrailed against just handing over the answer. It is a **service-layer feature** (`PRD §8`, `M2 §1`): the Rust core makes no LLM/network calls, the client packages student state and calls the hosted service, and the base learning loop must keep working with the tutor down.
>
> **This is a design, not a decision.** M2 already locked the consequential *service-layer* forks (hosted topology, Synapse identity, provider abstraction — `M2 §10`); this doc inherits those and does not re-open them. Where C2 introduces its *own* forks, it presents options + a recommendation and collects them in [§7 Open decisions for the owner](#7-open-decisions-for-the-owner).

---

## 0. What already exists in-tree (so this design builds on it, not around it)

M0–M2 shipped most of the read-model plumbing the tutor needs. Grounding the design in the actual tree (not the M2 doc's forward-looking sketch) sharpens what is genuinely new:

| Piece the tutor needs | Already in-tree | Where |
|---|---|---|
| Per-concept Memory (mean FSRS retrievability, `sufficient_data ≥ 3`) | **yes** — `Collection::concept_memory` + `ConceptMemory` RPC | `rslib/src/stats/concepts.rs`; `proto/anki/stats.proto:19`; `rslib/src/stats/service.rs:40` |
| Per-concept mastery summary (scored-card count, mean retrievability, `mastered` flag) | **yes** — `Collection::concept_mastery` → `ConceptMastery` | `rslib/src/storage/concept/mastery.rs:78` |
| Prerequisite graph (`from` is a prerequisite of `to`) + accessors | **yes** — `concept_edges` (schema 20), `get_prerequisites` / `get_dependents` | `rslib/src/storage/concept/edges.rs:96`,`:109` |
| Concept-graph node-link read-model (nodes coloured by Memory + directed edges) | **yes** — `Collection::concept_graph` + `ConceptGraph` RPC | `rslib/src/stats/graph.rs`; `proto/anki/stats.proto:22` |
| Per-concept Performance (F2), already rolling up an *aggregate* `prereq_mastery` scalar from `concept_edges` | **yes** — `ConceptPerformance` RPC (provisional/uncalibrated) | `proto/anki/stats.proto:23`,`:453` — proves the prereq rollup logic; the tutor needs the *per-prerequisite breakdown* it does not expose (§2.1) |
| The verified explanation for an item | **yes** — the `Explanation` field on the MCAT notetypes | `qt/aqt/synapse/provision.py:60` (`MCAT_FIELDS = ["Stem","Passage","Answer","Explanation","Concept"]`) |
| Miss detection at the reviewer | **yes** — `reviewer_did_answer_card` hook, capturing the just-answered card from the hook args | `qt/aqt/synapse/mint.py:62`,`:165` |
| A media-server-API-access web surface | **yes** — `AnkiWebViewKind.SYNAPSE` in `have_api_access`; `is_sveltekit_page`; `exposed_backend_list` | `qt/aqt/webview.py:146`; `qt/aqt/mediasrv.py:425`,`:752` |
| Card lineage (mint → source) as a queryable table | **yes** — `card_lineage` (schema 21), `relation` text column for future kinds | `rslib/src/storage/lineage/mod.rs`; `schema21_upgrade.sql` |

**Consequence for C2.** The tutor does **not** need to invent a mastery signal, a prerequisite graph, or a miss hook — those exist. It needs exactly two new things in the core, plus a service and a client panel:

1. one **new core read RPC** — a "concept + its weak prerequisites" *mastery bundle* — that composes `concept_memory`/`concept_mastery` with `get_prerequisites` so the client can assemble the state bundle in a single call (§2);
2. the **service tutor endpoint** and the **reviewer panel** (§3, §4).

Everything the tutor reads about the student already lives in queryable form; C2's core work is a thin roll-up, not new data.

---

## 1. The student-state bundle the CLIENT assembles at a miss (C2)

The PRD's C2 is a tutor that "knows my mastery map" and "surfaces the prerequisite I'm missing," grounded in "the verified explanation for the specific item" (`PRD C2`; `M2 §5.1`). The core makes **no network calls** — the *client* packages the state and calls the service (constraint from `M2 §1.1`, `§2`: no network in `rslib`).

### 1.1 What triggers it
A miss is an *Again* (ease 1) on an MCAT item — exactly the signal `mint.py` already keys on (`qt/aqt/synapse/mint.py:71`). The tutor reuses that hook (see §4), so "miss → tutor" and "miss → mint" fire off the same seam. The just-answered card is taken **from the hook's `card` argument, not `reviewer.card`** — by answer time the reviewer has advanced (documented at `mint.py:14`).

### 1.2 The bundle contents
On a miss the client gathers, entirely from local read-models (no DB access from the service — `M2 §5.1`):

| Bundle field | Source (in-tree) | Notes |
|---|---|---|
| **Just-answered card + notetype** | hook `card` arg (`reviewer_did_answer_card`) | identifies the item and confirms it is an MCAT notetype (`mint.py:76`) |
| **Concept tag(s) of the item** | the note's `concept::<section>::<id>` tags (mint already reads these — `mint.py:132`) | one item may carry several; the "primary" concept is the item's own `Concept`/tag |
| **The item's own Memory + `mastered` flag** | the mastery bundle RPC (§2), which wraps `concept_mastery`/`concept_memory` | mean retrievability + `sufficient_data`/`mastered` |
| **Weak prerequisites of that concept** | the mastery bundle RPC rolls up `get_prerequisites` (`edges.rs:96`) × mastery | the *unmastered* prereqs are the +2.7% lever (`PRD C2`) |
| **The verified explanation** | the item's `Explanation` field (`provision.py:60`) | the *answer-side grounding*; the tutor is grounded in this, exactly as C1 grounds generation in source text |
| **Recent answer history for the card (optional)** | `revlog` (available via existing `card_stats`/`get_review_logs`, `stats.proto:14`,`:15`) | lets the tutor say "you've missed this twice"; optional, small |

The bundle is **read-only client state** shipped to the service; the collection itself never leaves the device — only this small derived packet does (mirrors the F3 minimal-payload posture, `M2 §5.6`).

### 1.3 Why the *client* assembles it (not the service)
Constraint #1 of the whole architecture: `rslib` makes no network calls, and the service gets **no direct DB access** (`M2 §1.1`, `§2`, `§5.1`). The service could not query `concept_edges` or the revlog even if it wanted to — it has no collection. So the client (Qt Python on desktop; Kotlin on Android for the parity path, §4.3) does the reads through core RPCs and posts the assembled bundle over HTTPS. This keeps the grounding data-flow identical to C1: **the model only ever sees text the client retrieved and handed it.**

### 1.4 End-to-end (mirrors `M2 §8 trace B`)
Learner answers *Again* on an MCAT item → hook captures the card + concept tag(s) → client calls the mastery-bundle RPC (§2) for that concept, getting its Memory + the Memory/`mastered` of its prerequisites → client reads the item's `Explanation` → client POSTs `{concept, item_explanation, mastery_bundle, recent_history?}` to the tutor endpoint → service runs the answer-giveaway-guardrailed Socratic prompt grounded in that state → returns turns → shown in the reviewer at the point of error.

---

## 2. The one NEW core read RPC: a "concept + weak prerequisites" mastery bundle

C2 implies exactly **one** new core surface (the M2 appendix already named it: "mastery-bundle read RPC"). Everything else it reads already has an RPC. This section designs it; it is **not** implemented here.

### 2.1 Which service it appends to, and why append-only matters
It is a **read** over concept/mastery data that already lives beside `concept_memory`, `concept_graph`, and `concept_performance`, so it belongs on **`StatsService`** (`proto/anki/stats.proto:13`), next to its siblings. Service/method indices are **positional and shared across four languages**; inserting or reordering silently breaks Python/TS/Kotlin (`M2 §1.5`; roadmap "always *append*"; `ARCHITECTURE.md §9`). Today `StatsService` ends at:

```
rpc ConceptMemory(...)       // index 6  (F1 Memory)
rpc ConceptCoverage(...)     // index 7  (B4 coverage)
rpc AdoptionStats(...)       // index 8  (E2/E3)
rpc ConceptGraph(...)        // index 9  (D1 node-link)
rpc ConceptPerformance(...)  // index 10 (F2 Performance)  ← current last
```

The new RPC is **appended after `ConceptPerformance`** (it becomes index 11) — never inserted between existing ones. Adding it requires a full `just check` codegen pass (regenerates the Rust dispatcher + Python + TS + Kotlin bindings) and, on mobile, a matching backend-AAR bump or "mismatched proto indices fail silently" (`ARCHITECTURE_ANDROID.md §12`; `M2 §9`).

> **Why not reuse the existing `ConceptPerformance` RPC (index 10)?** F2 already computes a per-concept `prereq_mastery` scalar — the *mean* F1 Memory of a concept's prerequisites (`proto/anki/stats.proto:453`) — and already reads `concept_edges`. But it collapses the prerequisites into **one aggregate number** to *cap* the Performance score; it does not tell you **which** prerequisite is weak, its individual Memory, or whether it is `mastered`. The tutor needs the *breakdown, weakest-first*, to name the specific blocking concept (`PRD C2`'s +2.7% prerequisite-surfacing). So the mastery-bundle RPC is **complementary to F2, not duplicative**: same underlying `concept_edges` + retrievability math, different shape (per-prerequisite list vs. one capping scalar). It also confirms the rollup logic is already proven in-tree — this RPC is a re-shaping of code F2 exercises, lowering its risk further.

### 2.2 Request / response shape
The request names the concept(s) the client just missed on and scopes the card population the same way every other Synapse read-model does (`search`, e.g. `deck:Synapse`, empty = whole collection — matches `ConceptMemoryRequest.search`, `stats.proto:250`):

```proto
// APPENDED to StatsService after ConceptPerformance (positional index 11).
//   rpc ConceptMastery(ConceptMasteryBundleRequest)
//       returns (ConceptMasteryBundleResponse);

message ConceptMasteryBundleRequest {
  // The concept tag(s) the student just missed on, e.g.
  // ["concept::biochem::enzyme_kinetics"]. Usually one; a multi-concept item
  // may pass several. Unknown/untagged tags yield an empty entry (not an error).
  repeated string concepts = 1;
  // Card-population scope for the mastery/Memory rollup, e.g. "deck:Synapse";
  // empty = whole collection. Mirrors ConceptMemoryRequest.search so the tutor
  // and the dashboard agree on the same card set.
  string search = 2;
}

message ConceptMasteryBundleResponse {
  // One "focus" concept the student missed on, with its prerequisite context.
  message Bundle {
    // The missed concept itself, with its Memory/mastery signal (same numbers
    // the dashboard shows).
    ConceptState focus = 1;
    // Its DIRECT prerequisites (from concept_edges: `from` is a prereq of
    // `focus`), each with the same signal. Ordered weakest-first so the tutor
    // can surface the top unmastered prerequisite immediately.
    repeated ConceptState prerequisites = 2;
  }
  // One concept node's mastery state (a projection of ConceptMastery /
  // ConceptMemory — deliberately the SAME fields those already expose so the
  // tutor, dashboard and graph never disagree).
  message ConceptState {
    // full tag, e.g. concept::biochem::amino_acid_charge
    string concept = 1;
    // 2nd tag segment, e.g. biochem ("" if absent)
    string section = 2;
    // mean retrievability 0..100 over scored cards (0 when none scored)
    float memory = 3;
    // total cards mapped to this concept (coverage)
    uint32 card_count = 4;
    // cards with an FSRS memory state contributing to `memory`
    uint32 scored_card_count = 5;
    // scored_card_count >= 3 (the SUFFICIENT_DATA_THRESHOLD abstain gate)
    bool sufficient_data = 6;
    // clears the mastery threshold with enough scored cards
    // (ConceptMastery.mastered); when false AND has_cards, this is a candidate
    // "weak prerequisite" for the tutor to surface.
    bool mastered = 7;
    // false when the concept has no mapped cards: "nothing to study here" —
    // never treat as a blocking weak prerequisite (ConceptMastery::has_cards).
    bool has_cards = 8;
  }
  repeated Bundle bundles = 1;
}
```

### 2.3 How it rolls up `concept_memory` + `concept_edges` (no new data, just composition)
The Rust implementation (in a new `Collection::concept_mastery_bundle(...)`, e.g. `rslib/src/stats/concepts.rs` alongside `concept_memory`, or a small sibling module) is a **pure composition of functions that already exist**:

1. Resolve each requested tag → `ConceptId` via `get_concept_id_by_tag` (`concept/mod.rs:74`).
2. For each focus concept, read its **direct prerequisites** with `get_prerequisites(focus_id)` (`edges.rs:96`) — the `concept_edges` rows where the focus is the dependent `to`.
3. Compute the mastery signal for `{focus} ∪ prerequisites` in **one** `concept_mastery(&[ConceptId])` call (`mastery.rs:78`) — it already returns `total_card_count`, `scored_card_count`, `mean_retrievability`, `mastered`, and distinguishes `has_cards()` (`mastery.rs:55`). Map its `mean_retrievability` (0..1) to `memory` (0..100) and apply the same `sufficient_data = scored_card_count >= 3` gate `concept_memory` uses (`concepts.rs:16`,`:97`) so all three views agree.
4. Resolve ids → tags for display via `all_concepts()` (`concept/mod.rs:98`), exactly as `concept_graph` does (`graph.rs:58`).
5. Sort `prerequisites` **weakest-first** (unmastered-with-cards before mastered; ascending `memory` within), so `prerequisites[0]` is the tutor's headline "the thing actually holding you back" — the operationalization of Khanmigo's +2.7% prerequisite-surfacing (`PRD C2`).

This reuses the **exact** retrievability math three call-sites already share (`concept_memory`, `concept_mastery`, `concept_graph`), so there is no risk of the tutor's numbers drifting from the dashboard's. It is a read-only rollup: no `OpChanges`, no mutation, no schema change (the prerequisite graph and mastery projection are already schema 19/20).

> **Scope note (prereq depth).** The bundle returns **direct** prerequisites only (one hop). The tutor rarely needs a transitive chain for a single miss, and one hop keeps the payload small and the "top weak prerequisite" unambiguous. Transitive walking (multi-hop weakest-prerequisite) is a possible later extension over the same `get_prerequisites` accessor; it is flagged in §7 rather than built in, to keep the M3 surface minimal.

### 2.4 The `exposed_backend_list` + `is_sveltekit_page` wiring it needs
The tutor **panel** is a Svelte page (§4.2), and the browser can only reach **allow-listed** core methods (`M2 §1.3`; `ARCHITECTURE.md §5`). So the new RPC must be wired the same way `concept_memory`/`concept_graph` already are:

- **`exposed_backend_list`** (`qt/aqt/mediasrv.py:729`): append the method's snake_case name (`"concept_mastery"`) to the `# StatsService` block, right after `"concept_performance"` (the current last StatsService entry, `mediasrv.py:756`). Without this the page's `postProto` call 403s.
- **`is_sveltekit_page`** (`qt/aqt/mediasrv.py:413`): the tutor page's route name must be listed here (e.g. add `"tutor"` alongside `"synapse"`, `"coverage"`, `"graph"` at `mediasrv.py:425`) so mediasrv serves it as a SvelteKit page.
- **Android parity:** the NanoHTTPD `PostRequestHandler` allow-list mirrors `exposed_backend_list` (roadmap "NanoHTTPD `PostRequestHandler` allow-list mirrors `exposed_backend_list`") — the same method name must be allow-listed there for the mobile panel to read it.

> The **service** endpoints (tutor-turn, §3) are **service HTTPS API, not core proto** — they do **not** touch `exposed_backend_list` and do **not** require core codegen (`M2 §5.7`). Only the *read RPC that assembles the bundle* is a core surface; the tutor dialogue itself lives entirely off the proto contract.

---

## 3. The service tutor endpoint (HTTPS, per-user, server-side guardrails)

The tutor runs on the **hosted Synapse service** (Option B, locked in `M2 §2`/§10), reachable by the client shell over HTTPS with a short-lived bearer token derived from the Synapse identity (`M2 §3`). It never touches the collection; it consumes the bundle the client posts and returns dialogue turns.

### 3.1 Endpoint shape
A per-user, stateless-per-turn conversational endpoint (thread state kept server-side, `M2 §5.5` "tutor threads → service, per-user"):

- **`POST /v1/tutor/turn`** — request: `{ thread_id?, concept, item_explanation, mastery_bundle, recent_history?, user_message? }`; response: `{ thread_id, turns: [...], surfaced_prerequisite?, giveaway_blocked? }`.
  - On the **first** turn (fired automatically at the miss, §4), `user_message` is empty — the service opens Socratically ("Before the answer: what did you expect enzyme rate to do below Km?") using `mastery_bundle.prerequisites[0]` as the lever.
  - Subsequent turns carry the student's reply; the service continues, still grounded, still guardrailed.
- **Auth:** `Authorization: Bearer <token>` (per-user; `M2 §3`). Per-user thread history is keyed by `user_id` server-side; the collection stays on-device (`M2 §3`, `§5.5`).

### 3.2 Grounding contract (the C1 principle applied to dialogue — `PRD C2`)
The tutor is grounded in **two** things, both supplied by the client, never by the model's parametric memory:
1. **The verified answer** — the item's `Explanation` text (§1.2). The tutor may reason *from* it but is instructed not to *state* it (§3.3).
2. **The student's mastery map** — the `mastery_bundle`, so it targets the *actual* weak prerequisite rather than a generic hint.

This is the architectural expression of `PRD C2`'s finding: the tutor's value scales with grounding in (a) the correct answer and (b) the learner's mastery map — "the same grounding principle as C1 applied to dialogue" (`PRD C2`; `M2 §5.1`). A turn with **no** `item_explanation` and **no** `mastery_bundle` is refused server-side (nothing to ground on), mirroring C1's "no retrieval → refuse before the model" (`M2 §4.1`).

### 3.3 The answer-giveaway guardrail (server-side; §5 details enforcement)
Enforced **server-side** so the client cannot loosen it (`M2 §6`):
- **System prompt (primary):** the model is instructed to be Socratic — surface the unmastered prerequisite (`mastery_bundle.prerequisites[0]`), ask a leading question, give at most a partial hint, and **never emit the item's answer/answer key**. The `Answer` field is deliberately **not** sent to the tutor (only `Explanation`, which explains the reasoning) — reducing the surface for a leak at the source.
- **Optional output check (defense in depth):** a lightweight, rule-based post-generation check (string/normalized-match of the turn against the item's `Answer` — which the *service* can compare because the client may include a redacted `answer_hash` or the answer for check-only, never for prompting) can reject or regenerate a turn that leaks the answer, setting `giveaway_blocked=true`. This is the tutor analog of C1's rule-based-first gate (`M2 §4.3`: rules catch more than an LLM judge). It is **optional** and additive; the system prompt is the primary control.

### 3.4 Provider abstraction (inherited from M2, not re-decided)
The tutor calls the LLM through the **same `Generator` abstraction** M2 put in front of generation (`M2 §4.1`): provider, model id, and the tutor's prompt templates are config, not code, so the tutor can be A/B'd and swapped on price/quality. C2 adds no new provider decision; it consumes the one M2 already deferred to the owner (`M2 §10.4`).

---

## 4. Reviewer / client integration

### 4.1 Desktop: hook at the miss (reuse the mint pattern)
The tutor reuses the **exact** seam `mint.py` established (`qt/aqt/synapse/mint.py`), so nothing in `reviewer.py` is edited (the add-on convention, `M2`/roadmap "no tracked Anki source" where avoidable):

- Append a handler to **`gui_hooks.reviewer_did_answer_card`** (signature `(reviewer, card, ease)`; `mint.py:165`). On `ease == 1` **and** an MCAT notetype (`mint.py:71`,`:76`), the tutor is *offered proactively* — matching the PRD's central C2 design driver: Khanmigo's tutor sees only ~15% engagement precisely because it is a free-floating chat the student must remember to open, so **the tutor must show up at the moment of the mistake, in context** (`PRD C2` adoption caveat; Principle 6).
- **Capture the card from the hook's `card` argument, not `reviewer.card`** — the reviewer has already advanced by answer time (`mint.py:14`,`:16`). Same footgun, same fix.
- The offer is low-friction: a reviewer affordance (a button injected via the existing reviewer bridge, or a shortcut like mint's `Ctrl+M`, `mint.py:54`) that opens the tutor panel pre-seeded with the just-missed bundle. Proactive-but-dismissible: it appears, but never blocks the review flow (a student mid-crunch can ignore it), respecting the "base loop never depends on it" rule (§5).

### 4.2 The tutor panel MUST use `AnkiWebViewKind.SYNAPSE`
The panel is a SvelteKit page hosted in an `AnkiWebView`, exactly like the dashboard/coverage/graph dialogs (`qt/aqt/synapse/dashboard.py`). It **must** be constructed with `kind=AnkiWebViewKind.SYNAPSE` (`dashboard.py:55`). The reason is enforced in `webview.py`: only kinds in the `have_api_access` set get a profile whose `postProto` calls carry the auth token; `SYNAPSE` is in that set (`qt/aqt/webview.py:137`–`:146`), `DEFAULT` is not. A panel built as `DEFAULT` would **403 with "unexpected API access"** on its first core-RPC call (the mastery-bundle read) — the documented failure mode (`dashboard.py:52`–`:54`; and the standing memory note "Synapse SvelteKit dialogs need an `AnkiWebViewKind` in `have_api_access` or they 403").

Concretely, the tutor panel follows the `_SynapsePageDialog` pattern (`dashboard.py:31`): a `QDialog` owning an `AnkiWebView(kind=AnkiWebViewKind.SYNAPSE)` that calls `web.load_sveltekit_page("tutor")`. Unlike the three read-only dashboards, the tutor page **is interactive** — its bridge command handler (the `_on_bridge_cmd` that the read-only pages stub out, `dashboard.py:72`) forwards the student's typed replies to the service and streams turns back. The page reads the bundle via the new core RPC (§2) and talks to the tutor endpoint (§3) via the client shell (the Qt side makes the HTTPS call off the UI thread through `QueryOp`, `M2 §2` "client → service off the UI thread"; the page itself stays same-origin and only calls allow-listed core RPCs, since the service is a *different origin* the page cannot reach directly under the CSP-style allow-list, `M2 §1.3`).

### 4.3 Mobile parity (no add-ons)
AnkiDroid has **no add-on surface** and pre-renders cards; only the richer Svelte pages use the local NanoHTTPD server (`M2 §1.4`; `ARCHITECTURE_ANDROID.md §8`,§12). So the desktop hook mechanism does **not** port. The parity path:

- The **new read RPC (§2) is core Rust**, so it reaches Android **for free** through the `rslib-bridge` JNI shim (same `run_service_method` dispatcher) once the backend AAR is bumped to match the appended index (`M2 §5.7`,`§9`). The mobile client can assemble the same bundle.
- The **miss seam on mobile** is the client's own answer flow noticing the returned state (an *Again* on an MCAT card in Kotlin), not a Python hook — the same divergence M2 called out for leech repair (`M2 §5.3`: desktop `gui_hook` vs mobile "the client's own answer flow"). The tutor turn is then a **Kotlin coroutine HTTPS call** to the same service endpoint (`M2 §2` "client → service … Kotlin coroutine using the app's existing HTTP client"), and the tutor UI is a Svelte page served by NanoHTTPD, with the method allow-listed in the NanoHTTPD `PostRequestHandler` list that mirrors `exposed_backend_list` (§2.4).
- **M3 sequencing:** desktop-first (roadmap "Desktop-first"), so the tutor ships on Qt first; the shared read RPC lands in the same change so Android parity is a second wiring pass (a Kotlin miss-detector + panel), not a re-architecture.

---

## 5. Guardrails

The tutor's guardrails are **architectural**, not policies bolted on — the same posture M2 took for generation (`M2 §6`). Where each is *enforced*:

| Guardrail | PRD ref | Enforcement point |
|---|---|---|
| **No answer giveaway** | C2 | **Server-side** (client can't loosen it, `M2 §6`): (1) system prompt forbids emitting the answer and steers to the prerequisite; (2) the item's `Answer` field is **not** sent to the model (only `Explanation`); (3) *optional* rule-based output check rejects/regenerates a turn that matches the answer key, setting `giveaway_blocked` (§3.3) |
| **Grounded-only dialogue** | C2, Principle 4 | The tutor reasons only from client-supplied `item_explanation` + `mastery_bundle`; a turn with neither is refused before the model (§3.2) — the C1 "no grounding → refuse" rule applied to dialogue. No corpus/answer origination from parametric memory |
| **Surface the unmastered prerequisite** | C2 | The bundle returns prerequisites **weakest-first** (§2.3); the prompt uses `prerequisites[0]` as the lever — the operationalization of Khanmigo's +2.7% (`PRD C2`) |
| **Proactive, low-friction (fight the ~15% ceiling)** | C2, Principle 6 | Fires at the moment of the miss via the reviewer hook (§4.1), not a chat the student must open (`PRD C2` adoption caveat) |
| **Best-effort; degrade cleanly** | §8, roadmap | See §5.1 — the base loop never calls the tutor |

### 5.1 Degrade cleanly — the base loop must never depend on the tutor
The single hardest constraint (`M2 §9` "availability coupling"; roadmap "keep the core free of network I/O"; `PRD §8` the core owns data/scheduling, not external I/O). The tutor is **strictly additive**:

- **The M0–M2 loop never calls it.** Scheduling, minting (`mint.py`), the Memory/coverage/graph dashboards, and every core read-model work with the service down. The tutor is an *extra* offered at a miss.
- **Service-down UX:** if the tutor endpoint is unreachable or times out, the reviewer shows a quiet "tutor unavailable" state and the review continues; the student can still mint a card from the same miss (the mint path is 100% local, `mint.py`). No error blocks answering the next card.
- **Timeouts + off-UI-thread:** the HTTPS call runs off the UI thread (`QueryOp` on desktop, coroutine on Android, `M2 §2`) with a short timeout, so a slow service never freezes the reviewer.
- **The mastery-bundle RPC is local** (§2) — it works offline; only the *dialogue* needs the network. So even fully offline, the reviewer can still show "your weak prerequisite here is amino-acid charge" from the bundle alone, degrading from a Socratic dialogue to a static prerequisite hint.

---

## 6. Phasing + kill/quality criteria (from the PRD)

C2 is **M3 = PRD Phase 3** (`roadmap` M3; `PRD §9` Phase 3; `M2 §7` Phase 3). It depends on M2 having shipped the prereq graph (`concept_edges`, already in-tree, §0) and the service skeleton + provider abstraction + auth (`M2 §7`).

### Build order within M3 (dependency-driven)
1. **Mastery-bundle read RPC (§2)** — core, append to `StatsService`, wire `exposed_backend_list` + `is_sveltekit_page` + NanoHTTPD allow-list. Ships to Android for free. *Lowest-risk first: it is a pure read-model rollup over existing functions and needs no service.*
2. **Tutor endpoint (§3)** on the hosted service — reuse the M2 `Generator` abstraction + auth; system-prompt guardrail; optional output check.
3. **Desktop reviewer panel (§4.1–§4.2)** — reuse the mint hook + the `AnkiWebViewKind.SYNAPSE` dialog pattern; proactive-at-miss.
4. **Mobile parity (§4.3)** — Kotlin miss-detector + NanoHTTPD-served panel, after desktop proves out.

### Kill / quality criteria (carried verbatim from the PRD)
The PRD's C2 success bar is two-pronged (`PRD C2` "Success"; `M2 §7` Phase 3):

| Metric | Bar | Source |
|---|---|---|
| **Unaided next-item correctness lift** | Measurable lift for students who use the tutor after a miss — the genuine transfer metric (did they then solve the *next* item unaided, not AI-assisted). Khanmigo's grounded tutor produced **+6.1%** overall (+3.4% from history, **+2.7% from surfacing unmastered prerequisites**). | `PRD C2`; `M2 §7` |
| **Engagement well above the ~15% Khanmigo floor** | The tutor must be *adopted*. Khanmigo — an excellent grounded tutor — sees only **~15%** of eligible students engage; the proactive-at-miss design (§4.1) exists specifically to beat that floor. Adoption is a hard guardrail (`PRD §7`, Principle 6: "an app nobody opens teaches nothing"). | `PRD C2`, Principle 6; `M2 §7` |

**Kill/adjust posture.** If engagement does not clear the ~15% floor, the *surfacing* mechanism is re-tuned before the *dialogue* is blamed (the PRD's own diagnosis: the ceiling was a discoverability problem Khanmigo fixed with a proactive redesign, `PRD C2`). If unaided next-item correctness shows **no** lift over a no-tutor control, C2 does not graduate past experiment — consistent with the PRD's discipline of "build the experiment to check it rather than shipping the belief" (Principle 7). Note both are **transfer/adoption** metrics, not activity counts: measuring "tutor sessions" as an end would violate the §11 non-goal "optimizing engagement metrics … as ends in themselves."

### Ground-truth calibration link
"Unaided next-item correctness" from the tutor flow is exactly the ground truth the **Performance score (F2)** calibrates against (`PRD F2`; `M2 §7` Phase 3). C2 and F2 ship in the same phase and share this signal; the tutor's own outcome data feeds F2's calibration, so the tutor is both a feature and an instrument.

---

## 7. Open decisions for the owner

Ranked by how much they block C2. (The big service-layer forks — topology, identity, provider — were decided in `M2 §10`; C2 inherits them and does not re-open them.)

1. **Answer-giveaway output check: ship the optional rule-based post-check, or rely on the system prompt alone?** (§3.3, §5). The system prompt is the primary control and is enough to *design* against giveaway; the optional check is defense-in-depth with a small false-positive/regeneration cost. *Recommendation: ship the system-prompt guardrail first; add the rule-based output check as a fast-follow once real transcripts show whether leaks occur.* **Blocks nothing to start; decide before wide rollout.**
2. **Proactive trigger UX: auto-open the tutor panel at a miss, or offer a dismissible affordance the student taps?** (§4.1). Auto-open maximizes the anti-15%-ceiling effect but risks feeling intrusive mid-crunch (the overjustification/annoyance risk the PRD flags for engagement mechanics, `PRD E2/E3`). *Recommendation: dismissible affordance (button/shortcut, mirroring mint's `Ctrl+M`), proactive but never blocking — then A/B auto-open vs affordance against the engagement metric.* Product call.
3. **Does the tutor also offer a mint at the end of the dialogue?** (§1.4, §4.1). The miss seam already drives mint; chaining "tutor → mint a card from the clarified gap" is a natural, high-value loop (B1 + C2), but it couples two features. *Recommendation: keep them independent in M3 (both fire off the same hook), evaluate chaining after C2's engagement data.* Minor.
4. **Prerequisite depth: direct (one-hop) prerequisites only, or transitive weakest-prerequisite walking?** (§2.3). One hop keeps the payload small and the headline unambiguous and covers the common single-miss case; transitive walking could catch a deep root cause but adds cost/complexity to the read RPC. *Recommendation: one-hop for M3; revisit transitive walking if tutor transcripts show the true blocker is often two hops back.* Affects only the RPC's rollup, not its shape.
5. **Tutor thread persistence & privacy:** how long are per-user tutor threads retained server-side, and are they part of the F3-style consent posture? (§3.1; `M2 §3`,`§5.6`). Threads are per-user service data (not collection data), so they fall under the Synapse-identity consent gate M2 established — but retention/withdrawal policy for *dialogue* is a distinct legal/trust call. *Recommendation: minimize retention (keep only what the current thread needs); fold into the M2 consent policy.* Owner (legal/trust).
6. **Recent-history in the bundle: include per-card revlog, or concept mastery only?** (§1.2). Revlog lets the tutor say "you've missed this twice," a cheap engagement/relevance win, but it enlarges the payload and the on-device read. *Recommendation: include a tiny summary (lapse count / last few grades) via the existing `card_stats`/`get_review_logs` RPCs, not the full log.* Minor.

---

## Appendix — C2 feature → component map

| C2 concern | Where it lives | Core surface it uses | New work |
|---|---|---|---|
| Miss detection | desktop add-on hook (`mint.py` pattern); mobile answer-flow | `reviewer_did_answer_card` (desktop); `OpChanges`/answer result (mobile) | reuse; add a tutor handler |
| Student-state bundle | client (Qt Python / Kotlin) assembles it | mastery-bundle RPC (§2) + item `Explanation` + optional revlog | the RPC (§2) |
| "Concept + weak prerequisites" rollup | **core** read-model | `concept_memory`/`concept_mastery` + `get_prerequisites` (`concept_edges`) | **new `ConceptMastery` RPC**, appended to `StatsService` |
| Tutor dialogue | **hosted service** (`M2` Option B) | none (service HTTPS API, not core proto) | tutor endpoint (§3), reuses M2 `Generator` |
| Answer-giveaway guardrail | **service** (system prompt + optional output check) | — | server-side prompt/policy; optional rule check |
| Tutor panel | Svelte page in `AnkiWebView(kind=SYNAPSE)`; NanoHTTPD on mobile | allow-listed core RPCs only (§2.4) | `tutor` route; `exposed_backend_list`/`is_sveltekit_page`/NanoHTTPD entries |
| Degrade-cleanly | client | all base-loop RPCs are local (`mint`, dashboards) | best-effort call + "unavailable" state |

*Prepared for M3. This document is design only; no code is changed. Every architectural claim is grounded in `notes/prd.md` (C2, Principles 4 & 6, §11), `notes/M2_service_layer_design.md`, or the in-tree M0–M2 code cited inline (`rslib/src/stats/concepts.rs`, `rslib/src/stats/graph.rs`, `rslib/src/storage/concept/{edges,mastery}.rs`, `rslib/src/storage/lineage/mod.rs`, `proto/anki/stats.proto`, `rslib/src/stats/service.rs`, `qt/aqt/synapse/{mint,dashboard,provision}.py`, `qt/aqt/webview.py`, `qt/aqt/mediasrv.py`).*
