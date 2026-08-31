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

## Reconciliation candidate evidence (unpublished)

These results qualify the working candidate only. They do not promote a
support tier or substitute for verification of the actual release download.

| Target host | Observed execution | Remaining qualification |
| --- | --- | --- |
| macOS WKWebView | full arm64 application gate passed: controls, accessibility, typing, lifecycle and visual parity at 420x752 | exact release artifact and current-head CI |
| iOS WKWebView | isolated iPhone SE (3rd generation), iOS 26.5; Swift calls, publish, input/focus, assets and route state `7 -> 9 -> 7` passed | exact release artifact; physical-device support remains unclaimed |
| Android WebView | Android 15 emulator; Kotlin calls, publish, per-character input/focus with a delayed setter, service-worker retirement and packaged dynamic routes passed | exact release artifact; physical-device support remains unclaimed |
| Linux WebKitGTK | Rust companion compiles; shared route/security probe passes | Linux native GUI CI execution |
| Windows WebView2 | generated C compiles with official SDK; real C# protocol/OS execution | Windows native GUI CI execution |

Use `scripts/phase4-macos-test.mjs`, `scripts/ios/native-test.mjs`,
`scripts/android/native-test.sh`, `scripts/linux/native-test.sh` and
`scripts/windows/native-test.ps1` to reproduce each platform gate.

The final macOS/iOS/Android gates used the same frozen candidate executable.
Current-head Linux and Windows GUI CI is still pending; compilation and
protocol execution do not establish GUI qualification. API 36/Android 16
results below are historical and do not describe the final Android run.

An earlier unchanged Apple helper run timed out in three cases before later
runs passed. Android installation also needed an unchanged retry after a
boot-readiness failure. Their causes remain unconfirmed; passing reruns do
not establish a fix for either transient.

Android requires a System WebView supporting `WEB_MESSAGE_LISTENER`; the
AndroidX WebKit `1.14.0` bridge has no legacy fallback. An unsupported runtime
reports an unavailable bridge and needs a compatible WebView. Native calls'
ten-second browser wait limit does not preempt or roll back companion code;
companions must remain responsive, and a hung application may need relaunch.

## Historical Phase 5 Native Target Status

**Reconciliation notice (2026-08-30):** ADRs 0018 and 0019 replace the widget
planner and WebAssembly companions with platform web-view hosts and native
page companions. The tables below are historical evidence for the previous
architecture, not qualification of the new hosts. Fresh platform evidence is
required before publishing the reconciliation release; see
[`RECONCILIATION.md`](RECONCILIATION.md). No support tier is promoted by the
architecture change itself.

Evidence and reproduction commands are in `docs/PHASE_5_EVIDENCE.md`.

| Target | Status | Evidence |
| --- | --- | --- |
| macOS `SwiftUI` | native-tested development milestone | real `.app` launch, Accessibility, lifecycle, and screenshot parity |
| iOS `SwiftUI` | simulator-tested | signed simulator `.app` launched on iPhone 17 Pro / iOS 26.5; native button, text field, disclosure, isolated `WebSurface`, and lifecycle observed |
| Android views | simulator-tested (emulator) | debug APK launched on a headless `pixel_6` / Android 16 emulator; `uiautomator` confirmed native `Button` and `EditText`, declared content descriptions, native tap increment, and lifecycle |
| Linux GTK4 | container-tested | pinned Debian trixie container compiled the host with warnings denied, launched it headlessly, and read seven declared names, native roles, and an `Atspi.Action` activation back over AT-SPI |
| Windows Win32 | buildable with native execution evidence | `windows-latest` compiled, launched, inspected, drove, and closed the generated `PE32+` app; UIA names, a backing `Button` HWND, bound state, and lifecycle passed, while semantic UIA roles remain an explicit promotion gap |

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
permission to the process that runs the probe. Windows execution was recorded
by the `windows-native` job in [run 30714886154](https://github.com/d31ma/Tachyon/actions/runs/30714886154);
the target is not promoted because semantic UIA roles remain unproven.

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
| Eight-language Yon handler boundary | implemented milestone | mandatory five-layer stereotypes; explicit `@Relay`; direct `@Stream` in JavaScript, TypeScript, Python, PHP, Kotlin, and C#; Java and Rust return one response |
| Tac client rendering, components, mount schedules, and incremental builds | implemented milestone | evidence recorded in `docs/PHASE_3_EVIDENCE.md`; Yon remains a separate REST boundary |
| HTTP handler dispatch | implemented milestone | exact and dynamic routes, methods, parameters, bodies, middleware, and bounded topic streams execute through the supervised handler boundary |
| Environment-selected Yon isolation transport | implemented milestone | process compatibility mode and a fail-closed Firecracker control-driver transport for JavaScript and Python are tested; TypeScript and prepared direct-language artifacts are rejected before driver spawn until an artifact-transfer contract exists; the driver and Linux/KVM host are not yet qualified, so this is not a hardware-isolation support claim |
| macOS SwiftUI native vertical slice | native-tested development milestone | evidence recorded in `docs/PHASE_4_EVIDENCE.md`; ad-hoc development signing only |
| Android, iOS, Windows, and Linux native renderers | implemented | see the Phase 5 table above |

No Phase 4 artifact is a Tachyon preview or supported release. In default
process mode the handler child is supervised and receives a restricted
environment, but it still has the developer account's ambient filesystem and
network access. The Firecracker driver transport does not satisfy the OS-level
resource and capability isolation gate until its control program and host have
native qualification evidence. It also does not carry prepared TypeScript,
Java, PHP, Kotlin, C#, or Rust artifact sets; those language paths require
process mode for now.

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
