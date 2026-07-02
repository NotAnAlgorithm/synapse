# Android support (consolidated into the main Synapse repo)

This directory brings the **Synapse** Android app (the Kotlin/Android app,
derived from AnkiDroid) and its Rust JNI backend (**rsdroid**) into the main
Synapse monorepo, so Android is developed against the in-tree Rust core instead
of a pinned `anki` git submodule.

It is a faithful import of two upstream repos:

- `android/backend/` — from [Anki-Android-Backend](https://github.com/ankidroid/Anki-Android-Backend) (`rsdroid`)
- `android/app/` — from [Anki-Android](https://github.com/ankidroid/Anki-Android) (AnkiDroid)

## Layout

```
android/
  backend/   rslib-bridge/ (Rust JNI cdylib, crate "rsdroid"), build_rust/ (build orchestrator),
             rsdroid/ (Kotlin AAR), rsdroid-testing/ (host lib + RustBackendLoader for Robolectric),
             rsdroid-instrumented/, .cargo/config.toml, gradlew, settings.gradle.kts
  app/       AnkiDroid/ (app), libanki/ (Kotlin port of pylib/anki), api/, common/, compat/,
             anki-common/, lint-rules/, vbpd/, buildSrc/, gradlew, settings.gradle.kts
```

The Rust core (`rslib`, `proto`, …) lives at the repo root and is shared with the
desktop build. The backend bridge depends on it directly by path.

## Build topology (current: two independent Gradle builds)

`android/backend` and `android/app` are **separate Gradle builds** (each with its
own `gradlew`, `settings.gradle.kts`, and version catalog). The app consumes the
backend as the locally-built `.aar` + `.jar` **by default** (opt out to the published
library with `local_backend=false`). This is the least-invasive first step; collapsing
them into a single Gradle build (so `:AnkiDroid` depends on `project(":rsdroid")`
directly) is a planned follow-up that would remove the manual backend-build step.

## Prerequisites

- Rust toolchain (`rust-toolchain.toml`, 1.92.0), `cargo-ndk` 4.1.2
- Android SDK + NDK (version from `android/backend/gradle/libs.versions.toml`)
- JDK (17/21/25) for Gradle
- Node/Yarn (used by the desktop web build)

## Building an APK

The app builds against the **in-tree** Rust backend (the whole point of the monorepo).
The published `anki-android-backend` in the version catalog is pinned to an *older* Anki
release and will not match the in-tree core — e.g. it lacks newer proto enum values such
as `anki.decks.Deck.Filtered.SearchTerm.Order.RELATIVE_OVERDUENESS` — so the app now
**defaults to the locally-built backend**. Because of this, **you must build the backend
before the app.**

### 1. Build the backend (produces the rsdroid AAR + testing JAR)

```
cd android/backend
./build.sh                 # or: RELEASE=1 ./build.sh
```

`build.sh` first runs the repo-root `./ninja` (desktop web/proto build, producing
`out/rslib/proto/descriptors.bin`, `out/strings.json`, `out/extracted/protoc/…`, and the
web assets), then generates the protobuf Kotlin/Java + `GeneratedBackend.kt` /
`GeneratedTranslations.kt` from the in-tree `proto/anki/*.proto`, cross-compiles the JNI
bridge with `cargo-ndk` (+ a host lib for Robolectric), and assembles:

- `android/backend/rsdroid/build/outputs/aar/rsdroid-release.aar`
- `android/backend/rsdroid-testing/build/libs/rsdroid-testing.jar`

Re-run this whenever `proto/`, `ftl/`, or `rslib/` change, otherwise the app compiles
against stale generated code.

### 2. Build the app

```
cd android/app
./gradlew :AnkiDroid:assembleFullDebug
```

The debug APK lands under
`android/app/AnkiDroid/build/outputs/apk/full/debug/`.

Notes:
- There are three product flavors — `full` (FOSS), `play`, `amazon`. Use the
  flavor-specific task (`assembleFullDebug`, `assemblePlayDebug`, …). Plain
  `./gradlew assembleDebug` builds **all** flavors and is much slower.
- No `local.properties` entry is required — the in-tree backend is the default. To build
  against the published backend instead (rarely wanted here), set `local_backend=false`
  in `android/app/local.properties`.

### Troubleshooting

`Unresolved reference 'RELATIVE_OVERDUENESS'` (or any unresolved `anki.*` symbol) while
compiling `:libanki` / `:AnkiDroid` means the app is being compiled against a **stale or
published** backend that is older than the in-tree core. Fix: (re)run step 1 so the
in-tree `rsdroid-release.aar` exists and is current, then rebuild the app. Ensure
`local_backend=false` is **not** set in `android/app/local.properties`.

## What changed vs. the upstream repos

Only build wiring — no app/bridge logic was modified:

- **Submodule removed.** `android/backend/.gitmodules` deleted; the bridge's Cargo
  path deps (`anki`, `anki_proto`, `anki_io`, `anki_process`, `anki_proto_gen`,
  `anki_i18n`) now point at `../../../rslib*` (the in-tree core).
- `android/backend/.cargo/config.toml` env (`DESCRIPTORS_BIN`, `STRINGS_JSON_ANKIDROID`,
  `PROTOC`) repointed from `anki/out/…` → `../../out/…`.
- `android/backend/build_rust/src/main.rs` repointed from `anki/…` → `../../…`
  (and `current_dir("anki")` → `current_dir("../..")`) so it drives the repo-root ninja.
- `android/backend/rslib-bridge/proto.rs` protoc include/glob repointed to `../../../proto`.
- `android/app` backend dependency repointed from `../Anki-Android-Backend/…` →
  `../backend/…` (in `AnkiDroid/build.gradle` and `buildSrc/.../BackendDependencies.kt`),
  and the default flipped to the in-tree backend (the published catalog library, pinned to
  an older Anki release, is used only when `local_backend=false`).
- The main workspace `Cargo.toml` excludes `android/backend` (it is a self-contained
  Cargo workspace with its own lock + Android-only deps).

## Verification status

- ✅ Cargo graph resolves; the `anki` dependency resolves to the in-tree
  `rslib/Cargo.toml` (submodule fully severed).
- ✅ Root cause of the initial `:libanki` build failure identified and fixed: the app was
  compiling against the **published** backend (Anki 25.09.2), which is older than the
  in-tree core (26.05) and lacks `Order.RELATIVE_OVERDUENESS`. The app now defaults to the
  in-tree backend, so the documented flow (build backend → assemble app) compiles the
  correct generated code.
- ⏳ Still to be re-confirmed on a machine with NDK + JDK: `cargo ndk` cross-compile,
  Kotlin/protoc codegen, AAR/JAR assembly, full APK build, Robolectric tests.

## Follow-ups

- Collapse the two Gradle builds into one (`:AnkiDroid` → `project(":rsdroid")`),
  reconciling the two version catalogs.
- Promote the Kotlin/Fluent codegen into `rslib/proto` alongside `python.rs`/`typescript.rs`.
- Decide on Android CI. The nested `android/*/.github/workflows/` from the source
  repos are inert here (GitHub only runs the repo-root `.github/workflows/`).
