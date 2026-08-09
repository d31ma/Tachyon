# Handoff — Tachyon Rust Rewrite

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
export PATH="$HOME/.dotnet:$PATH"             # user-local .NET SDK with the net9 browser-wasm workload
export KOTLIN_WASM_STDLIB="$HOME/.local/share/tachyon/kotlin-stdlib-wasm-js-2.1.10.klib"
export ANDROID_HOME=/opt/homebrew/share/android-commandlinetools
export PATH="$ANDROID_HOME/emulator:$ANDROID_HOME/platform-tools:$PATH"
```

Toolchains installed on this machine, all verified working:

| Language | What it needs | Where it is |
| --- | --- | --- |
| Rust | `wasm32-unknown-unknown` | rustup 1.97.1 |
| Dart | nothing extra | 3.6.0 |
| Kotlin | `kotlin-stdlib-wasm-js.klib`, which `kotlinc` does not ship | `KOTLIN_WASM_STDLIB` above |
| Swift | a swift.org toolchain **and** the wasm SDK; Xcode's `swiftc` cannot cross-compile | Swift 6.3.3 installed user-locally through Swiftly and exposed through `~/Library/Developer/Toolchains/swift-6.3.3-RELEASE.xctoolchain`, plus `~/.swiftpm/swift-sdks` |
| C# | `wasm-tools` on .NET 9, or `wasm-tools-net9` on .NET 10+ | `~/.dotnet` (system .NET needs root, so it is unpatched) |

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
  (`RELEASED_TY_BIN=/path/to/v26.30.04/ty TY_BIN=/path/to/rewrite/ty node
  scripts/compat/differential.mjs`). The environment variables keep the gate
  tied to immutable binaries; no prior framework implementation remains in the
  tree.
- **Release lifecycle drill** PASS, including a bit-identical rebuild.

### What is open

`docs/CUTOVER.md` is the authority. Five of six gate conditions are met and one
is open:

1. **Release provenance (condition 4).** The workflow and local reproducible
   release drill pass, but one real tag-driven workflow must publish an archive
   and attestation before external verification can be recorded. The workflow
   stages and verifies the release privately before public promotion.

Separate target-promotion gaps remain:

- **macOS native evidence** is blocked on macOS Accessibility permission for
  the process that runs `node scripts/phase4-macos-test.mjs`. Grant it in
  System Settings and re-run. It is not a code problem.
- **Windows accessibility promotion** also remains open. Native execution now
  passes in CI, but the hosted managed UIA client flattens standard child
  controls to `ControlType.Pane`; semantic roles and `InvokePattern` still
  need evidence before `native-tested` promotion.

The stable workflow is tag-only and cannot create its own tag. It builds five
native CLI assets with auditable dependency metadata, publishes a CycloneDX
SBOM, attests and keyless-signs every distributed input, stages a private
draft, verifies the assets and installers on their native runners, and only
then makes the release public. `install.sh` and `install.ps1` now fail closed
on missing or mismatched checksums and install only the standalone `ty` binary.

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

- Structural control flow is server-owned. Static lists are authored in HTML;
  genuinely client-owned structure is created by an island's bounded
  `hydrate(root, signal)` implementation.
- Shared browser assets under `client/shared/` publish at the stable
  `/shared/` path. The compiler enforces file-count and byte limits and refuses
  links or non-files.
- The Rust compiler does not implicitly compose ancestor page layouts, so the
  website uses explicit `<atlas-layout>` and `<docs-layout>` components.
- The five polyglot examples are ordinary Rust, Dart, Kotlin, Swift, and C# in
  the ADR 0011 ABI shape. Each real-compiler sidecar remains paired with its
  legacy companion, so all five migrations classify as supported even though
  neither implementation consumes the other's source shape.
- Reused incremental routes retain their event descriptors, so a second build
  cannot delete `.tachyon/events.js`.

Evidence run on 2026-08-01:

```text
ty build website
  Built 11 routes ... compiled=0 reused=10
bun run test
  24 pass, 0 fail, 160 expectations
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
TY_BIN=target/debug/ty node scripts/compat/standalone-rust.mjs
  PASS: released standalone workflow matches Rust ty (macos)

RELEASED_TY_BIN=/tmp/.../26.30.04/ty TY_BIN=target/debug/ty \
  node scripts/compat/differential.mjs
  scaffold: 15 retained files byte-identical; 4 FYLO-facing files changed and 3 db files removed
  3/3 corpus projects match across implementations
```

The CLI now preserves the released version output, root help, target aliases,
comma-separated and `all` targets, target/host/port environment variables,
absolute `YON_DIST_PATH`, `serve --no-bundle`, source-only native packaging,
and cache shape. A real-binary regression proves `serve --no-bundle` does not
compile changed source. A second regression proves one multi-target command
publishes exactly one level at `dist/web`, `dist/macos`, `dist/ios`, and
`dist/android`; before that test existed, native targets were accidentally
published as `dist/macos/macos`.

The scaffold migration is explicit: 15 retained files remain byte-identical,
four FYLO-facing files change, and three legacy `db/` files are removed. The
released bounded page-class workflow
is also preserved: literal page state, class fields, `@onMount`, assignment
events, and native controller capabilities. Inline state is deliberately
literal-only; executable initializers fail with `TY1306` and are never emitted
as authored source. The browser gate caught and closed a compiler defect where
the route-level wrapper was emitted without `/.tachyon/islands.js`; page state
now has compiler-level emission coverage and repeated browser-level update
coverage.

The migrated website was then rebuilt through the same public commands:

```text
bun run bundle                         # Rust ty: 11 routes
bun run test                           # 24 pass, 160 expectations
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

The old in-tree JavaScript runtime and its implementation-coupled suite have
been removed. The archived 26.30.04 executable, standalone workflow, neutral
differential, and migrated website are the immutable public-behavior oracles.

## Recently fixed handoff defects

- Native-planning diagnostics now name `resolved/...` input when their offsets
  refer to a composed document; they no longer attach impossible ranges to an
  authored `tac.html` or `yon.html` file.
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

- Decisions that are theirs — hardware, credentials, risk acceptance, accepting
  simulator evidence — get recorded as **their** attributed, dated sign-off, not
  as a claim by the implementation. See the amendments in `docs/CUTOVER.md` and
  `docs/SUPPORT_TIERS.md` for the form.
- Prose is plain, specific, and admits what it does not know. Match it.
- Do not fabricate evidence to close a gate. The gate exists to be honest about
  what has not been proven.

## Suggested next moves

1. Complete the pull-request matrix and obtain review for the cutover commit.
2. Confirm the automated security matrix is clean and every critical or high
   finding in `docs/SECURITY_REVIEW_PACKAGE.md` has an owner and disposition.
3. Create the annotated `v26.32.07` tag only after the qualified commit is on
   `main`; the tag workflow will stage, verify, and publish it.
4. Grant macOS Accessibility permission and rerun the macOS evidence script.
