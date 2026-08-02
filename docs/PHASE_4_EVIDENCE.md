# Phase 4 macOS Native Vertical-Slice Evidence

## Result

Phase 4 is complete as a development milestone on 2026-07-26. It is not a
preview or supported release. The native-tested claim is limited to macOS
arm64; Android, iOS, Windows, and Linux native hosts remain Phase 5 work.

The normative contract is [`PHASE_4_SPEC.md`](PHASE_4_SPEC.md). The repeatable
host gate is `bun run test:rust-macos`; ignored run artifacts and their
SHA-256 digests are recorded under `target/phase4-evidence/`.

## Implemented Evidence

- `ty build PROJECT --target macos` consumes fully resolved Phase 3 HTML and
  atomically publishes `dist/macos`.
- Native UI v1 records fixed semantic adapters, deterministic route-local
  identities, accessibility roles/names, controller properties/events, and
  explicit local or remote WebSurface boundaries.
- Custom design-system elements select those adapters through explicit
  semantic roles rather than framework-specific tag names. The acceptance
  fixture's app bar declares `role="banner"`; unrecognised elements without a
  supported role remain isolated WebSurface subtrees.
- The generated SwiftUI/AppKit host compiles with the selected Xcode Swift
  toolchain, receives an ad-hoc code signature, and loads only bundled Native
  UI and route-index resources.
- HTML-authored scalar state supports text input/output, numeric increment,
  boolean toggle, and disclosure state without an application-authored Swift
  or JavaScript controller.
- Local fallback is subtree-scoped, CSP-constrained, non-persistent, and
  bridge-free. Remote fallback requires HTTPS, disables JavaScript, restricts
  navigation to the declared host, and is always bridge-free.
- Capability Manifest v1 denies all undeclared capabilities. Artifact Manifest
  v1 records release/source identity, target, Rust/Swift toolchains, contract
  versions, and every output path, size, and SHA-256 digest.
- Failed planning or host generation retains the previous complete
  application bundle.

## Real Application Acceptance

The committed `phase4-macos` fixture builds a 420 by 780 native application
with a native app bar, heading, text, button, output, text field, disclosure,
footer, and one isolated custom-element WebSurface.

The gate launches the compiled `.app`, reads its operating-system
Accessibility hierarchy, presses the AppKit button, types `Ada` with
CoreGraphics keyboard events, opens the disclosure, captures the native
window, and quits normally. Evidence proves:

- the output changes from `0` to `1`;
- the named text field receives `Ada` and logs `state.input`;
- the disclosure exposes its native content;
- required button, text-field, heading, group, and WebSurface names are
  present;
- lifecycle order includes `created`, `mounted`, `active`, state transitions,
  and synchronous `destroyed`.

The same resolved bundle runs at 420 by 780 in real Chromium. Its required
main, heading, button, status, textbox, disclosure, and fallback role/name
pairs match Native UI v1. The latest local comparison passed with vertical
transport distance `0.0768` (limit `0.16`), centroid distance `0.0711` (limit
`0.22`), and edge-density ratio `1.1419` (limit `4.0`).

## Adversarial and Recovery Evidence

Tests reject malformed/oversized/symlinked configuration, missing entry
routes, unsupported targets, inaccessible interactive controls, duplicate or
undeclared state, non-numeric increments, invalid verbs/names, unsafe remote
URLs, excessive nesting, excessive WebSurfaces, unresolved compiler syntax,
generated symlinks, unsafe output paths, and schema-invalid artifacts.

Native UI, Capability Manifest, and Artifact Manifest outputs validate against
their canonical Draft 2020-12 schemas. The capability test proves
`default_policy: deny`, an empty capability set, and
`remote_content_bridge: false`. The rollback test compares the executable
bytes before and after an intentionally failed rebuild.

## Repository Gates

The completion run includes formatting, all-target/all-feature checking,
strict Clippy, all 112 macOS-host Rust tests, warning-free rustdoc, the uniform
90% line/function/region coverage ratchet for target-neutral Rust, the Native
UI planner, and native configuration, dependency/license/source policy,
the released-binary compatibility differential, real Chromium islands, the real macOS parity
gate, a pinned Linux container run, Windows GNU cross-checking, and a release
binary smoke test reporting `ty 0.0.0-phase4`.

The pinned `rust:1.97.1-bookworm` Linux x86_64 container passed all 109 tests
applicable to that host; the omitted two are the explicit macOS bundle and
rollback tests. The historical JavaScript suite passed all 637 tests at the
Phase 4 checkpoint; it was removed when Rust became the sole in-tree
implementation.

The LLVM percentage excludes `native/compiler.rs` and `native/macos.rs`.
Those external Swift compiler, signing, and process-orchestration paths are
instead mandatory in an in-process Rust packaging test and the real launched
application gate; their Native UI inputs remain inside the 90% ratchet. The
passing report was 91.44% lines, 90.96% functions, and 91.38% regions.

These results qualify a Phase 4 development milestone only. Production
signing/notarization, installers, upgrades, rollback across releases,
entitlements, sandbox review, remote-origin live adversarial testing, fuzzing,
soak testing, and independent security assessment remain promotion gates.
