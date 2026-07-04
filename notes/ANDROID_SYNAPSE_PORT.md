# Synapse — Android (AnkiDroid) feature port

Ports the Synapse features added to the desktop app since Android support landed
(git `29098592d`) into the AnkiDroid app under `android/`. Reference for the
existing architecture: `notes/ARCHITECTURE_ANDROID.md`. Desktop source of truth:
`qt/aqt/synapse/`, `rslib/src/{scheduler,stats,storage}`, `backend/` (AI service).

## Porting model (three layers)

1. **Core Rust rides for free.** `android/backend/rsdroid` builds against the
   in-tree `rslib`/`proto` (Cargo path deps). The scheduler features
   (interleaving, mastery gating, trickle-down, metamorphosis, governor), the
   concept/lineage tables, the schema 18→21 migration, and the 7 new
   `StatsService` RPCs are all compiled in after a backend rebuild. **They are
   default-OFF**; provisioning writes the deck-config flags + `synapse:` config
   keys that activate them (same as desktop).
2. **Svelte dashboards reuse the desktop pages via WebView.** The `synapse`,
   `coverage`, and `graph` SvelteKit routes are bundled into the rsdroid AAR and
   served by the local NanoHTTPD `AnkiServer`, exactly like the existing
   deck-options page. Each is a `PageFragment`; the read-model RPCs they POST are
   registered in `PostRequestHandler.collectionMethods` (mirror of desktop
   `mediasrv.exposed_backend_list`).
3. **Native Kotlin for the rest** (provisioning, settings, minting, AI
   generation/tutor) — the desktop equivalents are Python/Qt, so they were
   reimplemented, sharing one constants file (`synapse/Synapse.kt`).

## What was added (Android)

- **Dashboards** (`pages/SynapseDashboard|SynapseCoverage|SynapseGraph.kt`) →
  Memory, Performance, Adoption/Effort, Coverage, Concept-graph. Reachable from
  the navigation drawer.
- **Backend passthroughs** (`libanki/.../stats/BackendStats.kt` +
  `pages/PostRequestHandler.kt`): `conceptMemory`, `conceptCoverage`,
  `conceptGraph`, `conceptPerformance`, `conceptMastery`, `adoptionStats`,
  `experimentMetrics`.
- **Provisioning** (`synapse/Provisioner.kt`, `synapse/SynapseSetup.kt`, nav
  "Synapse: Set up") — port of `provision.py`: MCAT notetypes (Grounding last),
  Synapse deck+preset with the 4 flags + retention 0.9 + RANDOM ordering, FSRS
  via the deck-options update flow, seed concepts. Idempotent.
- **Settings** (`preferences/SynapseSettingsFragment.kt`) — the three
  `synapse:service_*` keys + exam-date governor, written to collection config;
  cloud-sync URL cross-links to the existing custom-sync-server screen.
- **Reviewer miss-hook + minting + AI** (`synapse/SynapseReviewerHooks.kt`,
  `Minting.kt`, `SynapseAiClient.kt`, `GenerateReviewDialog.kt`, `TutorDialog.kt`,
  `SynapseGenerate.kt`): an `Again` miss on an MCAT card offers minting (offline)
  and/or the state-grounded tutor; grounded generation is reachable via a nav
  concept picker. OkHttp client to the Supabase Edge Functions (`/generate`,
  `/tutor`); tutor bundle assembled from the `conceptMastery` core RPC.

Shared names/fields/config-keys/classifiers live in `synapse/Synapse.kt`.

## Build / verify

```
cd android/backend && ./build.sh          # rebuild core AAR (proto+rslib codegen,
                                          # SvelteKit bundle, JNI, assemble AAR)
cd android/app && ./gradlew :AnkiDroid:assembleFullDebug
```

Verified in this environment: backend build `BUILD SUCCESSFUL`; the rebuilt AAR
contains all 14 new RPC methods; `:AnkiDroid:compileFullDebugKotlin` and
`assembleFullDebug` succeed → installable APKs for all four ABIs. An adversarial
review found no Critical issues and confirmed the AI contract, mastery-bundle
assembly, minting, provisioning, settings, and RPC wiring match desktop.

## Known follow-ups (parity gaps, non-blocking)

- **Generate-at-mastery auto-offer** (desktop offers generation on an Easy answer
  to a mature card) is not wired; generation is reachable via the nav picker.
- **Persistent reviewer context-menu** (desktop always exposes Mint/Generate/Tutor
  on the current card) is not ported; mint/tutor are offered at-miss.
- **Settings search**: `Preferences.getFragmentFromXmlRes` isn't mapped for the
  Synapse screen (navigation from the Settings list works; search result is a
  no-op). One line in `Preferences.kt`.
- Stray `android/backend/supabase/.temp/` (supabase CLI cache created in the wrong
  dir during development) should be removed / gitignored — not part of the port.
