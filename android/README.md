# Android support (consolidated into the main Anki repo)

This directory brings **AnkiDroid** (the Kotlin/Android app) and its Rust JNI
backend (**rsdroid**) into the main Anki monorepo, so Android is developed
against the in-tree Rust core instead of a pinned `anki` git submodule.

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
backend as a locally-built `.aar` + `.jar` via the `local_backend` flag. This is
the least-invasive first step; collapsing them into a single Gradle build (so
`:AnkiDroid` depends on `project(":rsdroid")` directly) is a planned follow-up.

## Prerequisites

- Rust toolchain (`rust-toolchain.toml`, 1.92.0), `cargo-ndk` 4.1.2
- Android SDK + NDK (version from `android/backend/gradle/libs.versions.toml`)
- JDK (17/21/25) for Gradle
- Node/Yarn (used by the desktop web build)

## Build flow

1. **Desktop build must run first.** The bridge consumes artifacts produced by the
   repo's own ninja build — `out/rslib/proto/descriptors.bin`, `out/strings.json`,
   `out/extracted/protoc/…`, and the web assets (reviewer.js, sveltekit pages,
   mathjax). `android/backend/build_rust` invokes the repo-root `./ninja` for these.
2. **Build the backend:** from `android/backend/`, run `./build.sh` (or
   `RELEASE=1 ./build.sh`). This: runs the desktop web/proto build, generates
   `GeneratedBackend.kt` + `GeneratedTranslations.kt`, cross-compiles the bridge
   with `cargo-ndk` into `.so`s (+ a host lib for Robolectric), and assembles
   `rsdroid/build/outputs/aar/rsdroid-release.aar` and
   `rsdroid-testing/build/libs/rsdroid-testing.jar`.
3. **Build the app:** create `android/app/local.properties` with `local_backend=true`,
   then `./gradlew :AnkiDroid:assembleFullDebug` from `android/app/`.

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
  `../backend/…` (in `AnkiDroid/build.gradle` and `buildSrc/.../BackendDependencies.kt`).
- The main workspace `Cargo.toml` excludes `android/backend` (it is a self-contained
  Cargo workspace with its own lock + Android-only deps).

## Verification status

- ✅ Cargo graph resolves; the `anki` dependency resolves to the in-tree
  `rslib/Cargo.toml` (submodule fully severed).
- ⏳ Not yet verified in this environment (needs NDK + JDK): `cargo ndk` cross-compile,
  Kotlin/protoc codegen, AAR/JAR assembly, APK build, Robolectric tests.

## Follow-ups

- Collapse the two Gradle builds into one (`:AnkiDroid` → `project(":rsdroid")`),
  reconciling the two version catalogs.
- Promote the Kotlin/Fluent codegen into `rslib/proto` alongside `python.rs`/`typescript.rs`.
- Decide on Android CI. The nested `android/*/.github/workflows/` from the source
  repos are inert here (GitHub only runs the repo-root `.github/workflows/`).
