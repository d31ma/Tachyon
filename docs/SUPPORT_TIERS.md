# Tachyon Support Tiers

## Vocabulary

- **Buildable**: the toolchain emits an artifact for the target.
- **Simulator-tested**: behavior passes in an emulator or simulator.
- **Native-tested**: the released artifact executes on the target operating
  system and architecture.
- **Preview**: the artifact is distributed for evaluation with documented
  missing production gates.
- **Supported**: installation, contracts, security, crash behavior, upgrade,
  rollback, accessibility, packaging, and soak gates pass on native
  infrastructure.

These labels are never interchangeable. Containers do not replace host-native
filesystem and process tests. Emulation does not replace device or native
architecture evidence for a supported claim.

## Phase 5 Native Target Status

Evidence and reproduction commands are in `docs/PHASE_5_EVIDENCE.md`.

| Target | Status | Evidence |
| --- | --- | --- |
| macOS `SwiftUI` | native-tested development milestone | real `.app` launch, Accessibility, lifecycle, and screenshot parity |
| iOS `SwiftUI` | simulator-tested | signed simulator `.app` launched on iPhone 17 Pro / iOS 26.5; native button, text field, disclosure, isolated `WebSurface`, and lifecycle observed |
| Android views | simulator-tested (emulator) | debug APK launched on a headless `pixel_6` / Android 16 emulator; `uiautomator` confirmed native `Button` and `EditText`, declared content descriptions, native tap increment, and lifecycle |
| Linux GTK4 | container-tested | pinned Debian trixie container compiled the host with warnings denied, launched it headlessly, and read seven declared names, native roles, and an `Atspi.Action` activation back over AT-SPI |
| Windows Win32 | buildable | `PE32+` GUI binary cross-compiled with warnings denied; execution is gated by the `windows-native` CI job, which has not yet reported |

No Phase 5 target is `supported` in the vocabulary above, and the vocabulary
has not moved: simulator and emulator evidence still does not establish
physical-device behavior, and a container still does not establish Linux host
behavior.

**Amendment, 2026-07-31 (Samuel Ezenma, maintainer):** the *cutover gate*
accepts simulator, emulator, and pinned-container evidence for its native
evidence condition. That is a decision about what blocks cutover, not a
promotion: a target reaches `supported` only through the promotion evidence
below. See [`CUTOVER.md`](CUTOVER.md), which records the same amendment and
what it does not establish.

Re-verified 2026-07-31 by execution: iOS on iPhone 17 Pro / iOS 26.5, Android
on the `tachyon-gate` emulator (API 36), and Linux in the pinned
`debian:trixie-slim` image. macOS is blocked on granting Accessibility
permission to the process that runs the probe; Windows execution remains
gated by the `windows-native` CI job, which has still not reported.

The migrated 11-route website was additionally rebuilt and launched on
2026-08-01. The iOS simulator and macOS application rendered the complete
homepage after their isolated surfaces settled. The Android emulator rendered
the same mobile layout; physical emulator taps navigated `/` to `/docs` and
then resolved the dynamic `/docs/introduction` route through the native route
stack. Its mobile menu, theme control, and light system-bar appearance were
also exercised on the rebuilt APK with no fatal application log entry. This
adds application-level evidence, but it does not promote any tier.

The same website source also bundled successfully with the archived legacy
26.30.04 binary using the ordinary `ty bundle` command and an isolated
`YON_DIST_PATH`; both implementations published the same 11 authored HTML
routes. Browser semantic parity remains governed by the differential corpus.

## Phase 4 Status

| Surface | Status | Reason |
| --- | --- | --- |
| Rust workspace and Phase 4 CLI on macOS arm64 | native-tested development milestone | real binary, HTTP, Node.js, CPython, Chromium, SwiftUI application, Accessibility, lifecycle, and screenshot evidence executed locally |
| Rust workspace and Phase 4 CLI on Linux x86_64 | container-tested development milestone | locked platform-neutral Rust/process suite executes in a pinned official Linux container; macOS packaging correctly remains host-gated |
| Rust workspace and Phase 4 CLI on Windows x86_64 | buildable | GNU target compiles locally; real Node.js and CPython execution remains enforced by `windows-latest` CI |
| Static web compiler and development server | implemented milestone | evidence recorded in `docs/PHASE_1_EVIDENCE.md`; not published |
| JavaScript and Python Yon handler boundary | implemented milestone | evidence recorded in `docs/PHASE_2_EVIDENCE.md`; explicit CLI/library invocation only |
| Tac/Yon view context, components, islands, and incremental builds | implemented milestone | evidence recorded in `docs/PHASE_3_EVIDENCE.md`; build-time prerendering only |
| HTTP handler dispatch | implemented milestone | exact and dynamic routes, methods, parameters, bodies, middleware, and bounded topic streams execute through the supervised handler boundary |
| macOS SwiftUI native vertical slice | native-tested development milestone | evidence recorded in `docs/PHASE_4_EVIDENCE.md`; ad-hoc development signing only |
| Android, iOS, Windows, and Linux native renderers | implemented | see the Phase 5 table above |

No Phase 4 artifact is a Tachyon preview or supported release. The handler
child is supervised and receives a restricted environment, but it still has
the developer account's ambient filesystem and network access. OS-level
resource and capability isolation is a future qualification gate.

## Promotion Evidence

A target cannot become supported without:

- clean installation from the release archive;
- artifact digest, signature, SBOM, and provenance verification;
- CLI and contract corpus execution;
- paths containing spaces, Unicode, and platform separators;
- filesystem permission and symlink behavior;
- child-process cancellation, crash, signal, and reaping behavior;
- native UI interaction and accessibility evidence where applicable;
- upgrade from every supported release line;
- safe rollback or an explicit irreversible-migration boundary;
- bounded load and soak results on named reference hardware.
