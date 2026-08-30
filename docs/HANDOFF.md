# Handoff — Tachyon Rust Rewrite

> Current architecture override (2026-08-10): ADR 0015 makes Tac exclusively
> client-rendered. Historical SSR-island notes below describe superseded
> evidence and must not be used as implementation authority. ADR 0016 makes
> Yon REST-only; it has no templates or server renderer.

You are taking over the Rust rewrite of Tachyon. This is the whole briefing:
what is true today, what is left, where the traps are. Everything here was
verified by running it, not by reading it. Where something is unverified, it
says so.

## Your role

Framework engineer across compiler, server runtime, native hosts, packaging,
and documentation. `AGENTS.md` defines the canonical gate and the rewrite
boundary; `docs/ENGINEERING_STANDARDS.md` defines the bar. Read `CONTEXT.md`,
`docs/PROJECT_PLAN.md`, `docs/PARITY_LEDGER.md`, and `docs/CUTOVER.md` before
changing anything.

## The one rule that matters here

**Verify by execution. Never by inspection, and never by assertion.**

This repository's documentation is unusual: every claim carries the command
that produced it. That is not decoration — it is the reason the parity ledger
can be trusted. If you cannot run something, say so and record it as unknown.
An unknown is not a pass. Do not write "met", "verified", or "passing" next to
anything you did not watch succeed.

Two ways this bit me, so it will bite you:

- `cargo test --workspace 2>&1 | grep -E "FAILED|panicked"` returns **nothing**
  when the test binaries fail to *compile*. I reported a green suite twice on
  that basis and was wrong both times. Count the result lines instead:
  `cargo test --workspace 2>&1 | grep -cE "^test result"` should be 17, and
  check separately for `^error(\[|:)`.
- A gate script that is not executable in git fails only on a clean checkout.
  All of `scripts/**/*.sh` had mode 644 and the release lifecycle drill could
  never have run in CI. Fixed, but look for the class of thing.

## Environment

Work in `~/dev/TACHYON`, **not** the Dropbox copy. Dropbox syncs the build
cache and makes git operations take minutes.

```bash
export PATH="$HOME/.cargo/bin:$PATH"          # a Homebrew rustc 1.95 shadows rustup 1.97.1
export PATH="$HOME/dev/TACHYON/node_modules/.bin:$PATH"  # tsc 7.0.2; the one on PATH is 5.1.3
export PATH="$HOME/.dotnet:$PATH"             # user-local .NET 9 with wasm-tools; the system one has no workload
export TAC_KOTLIN_WASM_STDLIB="$HOME/.local/share/tachyon/kotlin-stdlib-wasm-js-2.1.10.klib"
export ANDROID_HOME=/opt/homebrew/share/android-commandlinetools
export PATH="$ANDROID_HOME/emulator:$ANDROID_HOME/platform-tools:$PATH"
```

Toolchains installed on this machine, all verified working:

| Language | What it needs | Where it is |
| --- | --- | --- |
| Rust | `wasm32-unknown-unknown` | rustup 1.97.1 |
| Dart | nothing extra | 3.6.0 |
| Kotlin | `kotlin-stdlib-wasm-js.klib`, which `kotlinc` does not ship | `TAC_KOTLIN_WASM_STDLIB` above |
| Swift | a swift.org toolchain **and** the wasm SDK; Xcode's `swiftc` cannot cross-compile | Swift 6.3.3 installed user-locally through Swiftly and exposed through `~/Library/Developer/Toolchains/swift-6.3.3-RELEASE.xctoolchain`, plus `~/.swiftpm/swift-sdks` |
| C# | `wasm-tools` workload | `~/.dotnet` (system .NET needs root, so it is unpatched) |

`ty doctor <project>` reports all five and probes capability, not just
`--version`. Trust it over your assumptions: four of these looked ready when
none could build.

## State of the repository

The rewrite and website migration are being prepared on
`codex/stable-rust-rewrite` as one cutover release candidate. Review `git
status` before committing: it contains the greenfield Rust rewrite, earlier
product changes, the website cutover, and release hardening, not one isolated
patch.

### What is done and proven

- **Phases 0–7 of the engineering work.** Evidence per phase in
  `docs/PHASE_*_EVIDENCE.md`.
- **Browser companions in five languages** — Rust, Dart, Kotlin, Swift, C# —
  each compiled by its own real compiler, none by a subset transpiler. One JSON
  protocol, two module shapes (bare wasm; a JS module exporting `tacInvoke`
  where the toolchain emits only WasmGC or a whole runtime). See ADR 0011 and
  its amendment. Gate: `node scripts/wasm/companion-browser-test.mjs` drives all
  five on one page in Chromium.
- **Native apps for iOS, Android, and Linux**, re-verified 2026-07-31 by
  execution. `scripts/android/native-test.sh` and `scripts/linux/native-test.sh`
  both PASS; iOS was driven by hand on iPhone 17 Pro. The macOS evidence block
  is named below.
- **Windows native execution** in CI run 30714886154. The generated Win32 app
  compiled, launched, exposed its UIA names and backing `Button` HWND, accepted
  `BM_CLICK`, changed bound state `0 → 1`, recorded lifecycle, and closed.
- **Compatibility differential** 3/3 corpus projects
  (`TAC_LEGACY_BIN=/path/to/released/ty TAC_BIN=/path/to/rewrite/ty node
  scripts/compat/differential.mjs`). The environment variables keep the gate
  tied to immutable binaries even after the in-tree legacy runtime is removed.
- **Release lifecycle drill** PASS, including a bit-identical rebuild.

### What is open

`docs/CUTOVER.md` is the authority. Five of six gate conditions are met and one
remains open. Automated security qualification is mandatory for each release;
an independent human review is optional and nonblocking:

1. **Release provenance (condition 4).** The workflow and local reproducible
   release drill pass, but one real tag-driven workflow must publish an archive
   and attestation before external verification can be recorded.

Separate target-promotion gaps remain: macOS Accessibility permission is still
needed for the local probe, and Windows semantic UIA roles/`InvokePattern`
remain unproven even though native execution passed.

The stable workflow is tag-only and cannot create its own tag. It builds five
native CLI assets with auditable dependency metadata, publishes a CycloneDX
SBOM, attests and keyless-signs every distributed input, stages a private
draft, verifies the assets and installers on their native runners, and only
then makes the release public. `install.sh` and `install.ps1` now fail closed
on missing or mismatched checksums and no longer install removed FYLO tooling.

## Website migration — completed 2026-08-01

`website/` now builds and tests through the Rust implementation. It emits 11
routes and compiles the five real-language browser companions with their own
toolchains. The migration check reports **128 supported, 1 changed, 1
unsupported**. The five ADR 0011 companion migrations are supported because
each legacy companion is paired with a real-compiler sidecar; cross-document
navigation is the single changed finding. The remaining
unsupported finding is telemetry, an explicit product boundary in the parity
ledger.

The migration establishes these authoring rules:

- Structural control flow is authored in Tac HTML and owned by the client
  renderer. The compiler serializes bounded expressions and control plans;
  mounted component companions supply state and methods. The legacy
  `hydrate` spelling is only a mount-schedule alias, not an imperative
  structure escape hatch.
- Shared browser assets under `client/shared/` publish at the stable
  `/shared/` path. The compiler enforces file-count and byte limits and refuses
  links or non-files.
- The Rust compiler does not implicitly compose ancestor page layouts, so the
  website uses explicit `<atlas-layout>` and `<docs-layout>` components.
- The five polyglot examples are ordinary Rust, Dart, Kotlin, Swift, and C# in
  the ADR 0011 ABI shape. They are deliberately migration findings because the
  legacy subset transpiler cannot consume the same sources.
- Reused incremental routes retain their event descriptors, so a second build
  cannot delete `.tachyon/events.js`.

The build, companion, and legacy-oracle evidence below was recorded on
2026-08-01; the website suite was re-run on 2026-08-11:

```text
ty build website
  Built 11 routes ... compiled=0 reused=10
bun run test
  24 pass, 0 fail, 353 assertions
node scripts/wasm/companion-browser-test.mjs
  wasm companion gate passed for rs, dart, kt, swift, cs

YON_DIST_PATH=<temporary-output> /path/to/26.30.04/ty bundle
  Bundle completed ... routes=66 targets=web
  11 authored route HTML files emitted beneath web/
```

The last command ran from `website/` with the archived 26.30.04 executable.
The inflated legacy log count includes its internal page/component entries;
the published authored route set is the same 11-route set produced by the
rewrite. Using an isolated `YON_DIST_PATH` kept the two outputs independent.

The event, island-expression, service-worker, and Phase 3 Chromium gates also
pass. Repeated reused builds retain byte-identical `.tachyon/events.js` and
`shared/scripts/imports.js`.

## Released-surface parity hardening — completed 2026-08-01

The public workflow is now exercised directly against the Rust executable,
not inferred from command aliases. `scripts/compat/standalone-rust.mjs` creates
a project, verifies every scaffold file, bundles and serves web output, checks
the released cache lifecycle and `spa-renderer.js` entry, builds a source-only
native host, drives literal page state through a real Chromium click, and
executes its native controller through state, shortcut, and content-surface
calls.

```text
TAC_BIN=target/debug/ty node scripts/compat/standalone-rust.mjs
  PASS: released standalone workflow matches Rust ty (macos)

TAC_LEGACY_BIN=/tmp/.../26.30.04/ty TAC_BIN=target/debug/ty \
  node scripts/compat/differential.mjs
  scaffold: 18 generated files match after declared legacy scaffold removals
  3/3 corpus projects match across implementations
```

The CLI now preserves the released version output, root help, target aliases,
comma-separated and `all` targets, target/host/port environment variables,
absolute `TAC_DIST_PATH`, `serve --no-bundle`, source-only native packaging,
and cache shape. A real-binary regression proves `serve --no-bundle` does not
compile changed source. A second regression proves one multi-target command
publishes exactly one level at `dist/web`, `dist/macos`, `dist/ios`, and
`dist/android`; before that test existed, native targets were accidentally
published as `dist/macos/macos`.

The 18 parity-covered scaffold files remain byte-identical, including the root
Yon handler. The declared differences remove the obsolete database layout and
environment entries, normalize the standalone-only installer sentence, and
exclude the replacement standalone ambient type contract from byte parity; its
generation remains covered by Rust scaffold tests. The released bounded
page-class workflow is still preserved:
literal page state, class fields, `@onMount`, assignment events, and native
controller capabilities. Inline state is deliberately literal-only;
executable initializers fail with `TY1306` and are never emitted as authored
source. The current browser gate requires a schema-versioned render plan and
`/.tachyon/tac-client.js` for every Tac route, with compiler-level emission
coverage and repeated browser-level update coverage.

The migrated website was then rebuilt through the same public commands:

```text
bun run bundle                         # Rust ty: 11 routes
bun run test                           # 24 pass, 353 assertions
YON_DIST_PATH=<temp> /path/to/26.30.04/ty bundle
                                       # 11 authored route documents
ty bundle --target macos,ios,android
                                       # three packaged 11-route apps
```

The combined native build reported 296 native nodes and 39 bounded surface
payloads per target. The corrected artifacts installed and launched on macOS,
the booted iPhone 17 Pro simulator, and the Android emulator. Android CDP
proved the mobile menu opens and closes and the persisted theme changes from
light to dark; app-PID logs had no fatal exception. The native Android gate
also passed platform-widget accessibility, increment interaction, isolated
fallback, and lifecycle logging. iOS and macOS rendered the same responsive
view in screenshots; iOS needed roughly 5–15 seconds for its initial local
WebKit surfaces on this simulator. Automated clicks on those two hosts remain
blocked by the macOS assistive-access permission named elsewhere in this file,
so that interaction is not claimed from the screenshots.

The in-tree JavaScript framework and its test suite were removed on 2026-08-09.
The archived 26.30.04 executable, standalone workflow, neutral differential
corpus, and migrated website are the immutable public-behavior oracles. A
repository policy test prevents the deleted implementation paths from
returning.

## Recently fixed handoff defects

- Native-planning diagnostics now attach to authored `tac.html` input. Yon does
  not participate in native view planning.
- Same-origin links inside a local WebSurface now hand navigation to the native
  route stack. Android emulator taps executed `/` → `/docs` →
  `/docs/introduction` on the migrated website.
- Native planning no longer hardcodes `w-app-bar` or `w-button`. Custom design
  systems reach adapters through explicit semantic roles; the Phase 4 fixture
  declares `role="banner"` and keeps its `layout.app_bar` evidence honestly.

## Traps in the code

- **`n_%06d` node ids are a public contract** (`api/native-ui/v1/schema.json`,
  `^n_[a-z0-9_]{1,126}$`). I broke it scoping ids per route; the schema test
  caught it. Scope the *payload path*, not the id.
- **SwiftUI `@StateObject` makes a view's memberwise init main-actor isolated**,
  which breaks the nonisolated node builder in the generated host. Use
  `sizeThatFits` instead of an observable height.
- **The generated Swift, Java, and C hosts are Rust string literals.** They are
  compiled only when you build for that target, so a typo is invisible to
  `cargo test`. Build the real app for every host you touch.
- **Android emulator**: a killed emulator leaves `*.lock` files in
  `~/.android/avd/<name>.avd/` and the next boot dies with "a snapshot operation
  is pending". Delete them and boot with `-no-snapshot`.
- **Docker on this machine** cannot use buildx (`~/.docker/buildx/refs` is
  root-owned); build the Linux gate image with `DOCKER_BUILDKIT=0`.
- **Disk is tight.** Emulators refuse to boot under ~3 GB free. `target/` is a
  cache and safe to delete; nothing unique lives there.

## How the maintainer works

- Decisions that are theirs — hardware, credentials, security review, accepting
  simulator evidence — get recorded as **their** attributed, dated sign-off, not
  as a claim by the implementation. See the amendments in `docs/CUTOVER.md` and
  `docs/SUPPORT_TIERS.md` for the form.
- Prose is plain, specific, and admits what it does not know. Match it.
- Do not fabricate evidence to close a gate. The gate exists to be honest about
  what has not been proven.

## Suggested next moves

1. Complete the pull-request matrix and obtain review for the cutover commit.
2. Create the annotated `v26.35.07` tag only after the reviewed commit is on
   `main`; the tag workflow will stage, verify, and publish it.
3. Grant macOS Accessibility permission and rerun the macOS evidence script.
4. Schedule an independent human security review for major trust-boundary
   changes when useful; it is not a publication prerequisite.

## Semantic hot updates — completed 2026-08-09

The development loop no longer fingerprints the source tree every 400 ms.
`notify` feeds a bounded queue and 75 ms quiet-period coalescer; successful
builds publish Hot Update Protocol v1 at `/.tachyon/hot`. CSS is swapped in
place, supported Tac companion edits replace compiler-owned island boundaries,
and all structural or uncertain edits reload. Failed builds leave the previous
output and DOM running while a text-only structured diagnostic overlay is
shown.

Island instances can implement `hotState()`, `restoreHotState(state)`, and
`hotDispose()`. Without explicit state hooks, enumerable non-function fields
are copied with `structuredClone`; a failed default clone retains an empty
object. An explicit `hotState()` result is also cloned before restore. No JSON
or depth bound is claimed. Disposal always aborts the runtime-owned signal.
Missed event sequences, broadcast lag, changed boundary identity, and client
errors widen to a reload rather than attempting an unsafe patch.

Evidence lives in `crates/tachyon-core/tests/dispatch.rs`, compiler and watcher
unit tests, and `scripts/hot-update-browser-test.mjs`. The browser scenario was
also run interactively against `ty dev`: CSS and island changes retained a
counter value, the old island disposed and aborted, an invalid template showed
diagnostics without replacing the page, and restoring the structural template
caused the expected full reload with no console errors.

The real Tachyon website follow-up found and closed two gaps. Static generated
assets now win before a dynamic page fallback, so `/docs/client.js` is served
as JavaScript instead of being rewritten to `/docs/_topic`; `/docs` again
redirects and renders Introduction. Island replacement now retains a bounded
set of native browser state in addition to companion state: form values and
checked state for inputs, `<details>` disclosure, nonzero scroll offsets, and
focus by element id. The snapshot considers at most the first 2,048 elements
with an `id`; it does not retain text selection or selected `<option>` state.
Real-browser verification retained an open quickstart disclosure and a focused
edited input through a live companion replacement without console errors.

## Environment-selected Yon isolation — added 2026-08-09

ADR 0014 adds a provider boundary without placing deployment policy in
`tachyon.json`. `YON_ISOLATION=process` is the compatibility default;
`firecracker` requires an absolute, regular, non-symlinked control program and
passes bounded pool, vCPU, memory, and deny-egress arguments to it. Every Yon
entry point reads the same parent environment and uses Handler Protocol v1,
including middleware, workers, HTTP dispatch, and explicit CLI
invocation. Invalid or partial configuration fails as `TY2010` before handler
execution.

Do not describe this as qualified microVM isolation yet. The first-party
control program, Linux/KVM host profile, jailer configuration, guest image,
warm-pool and snapshot lifecycle, and native adversarial evidence still need
their own vertical slice.
