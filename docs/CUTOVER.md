# Cutover Gate Status

The cutover gate in [`PROJECT_PLAN.md`](PROJECT_PLAN.md) lists six conditions
that must all hold before the Rust implementation becomes Tachyon's default.

**Five conditions are met and one remains open.** Automated security
qualification is the pre-tag security gate; an independent human review is
optional and does not block merge, tagging, or publication. The remaining
open condition requires one real release-workflow run so the repository
produces externally verifiable release artifacts and attestations.

Every claim below is either an execution result with the command that produced
it, or a maintainer decision with the name of the person who made it. Nothing
here is an assertion by the implementation about itself.

## Status

| # | Condition | Status | Basis |
| --- | --- | --- | --- |
| 1 | Every stable behavior has passing compatibility evidence or a documented migration | **Met** | Execution, re-verified 2026-07-31 |
| 2 | Supported platforms pass their native evidence profiles | **Met**, under the evidence standard amended below | Execution on three platforms, 2026-07-31; macOS and Windows named below |
| 3 | Install, upgrade, rollback, and uninstall have been exercised | **Met** for the `ty` artifact | Execution, re-verified 2026-07-31 |
| 4 | Release artifacts are signed, attested, and independently verifiable | **Open** | Repository work complete; awaits one real workflow run |
| 5 | No unowned critical threat-model finding remains | **Met** | Automated technical audit, remediation, and exact-head enterprise qualification; no critical or high blocker remains |
| 6 | Stable documentation describes the Rust implementation rather than plans | **Met** | Repository and website documentation rewritten and checked against executable behavior, 2026-08-01 |

## Evidence Standard — amended 2026-07-31

The original gate required physical-device runs for iOS and Android, a
host-native Linux runner, and a recorded `windows-native` CI run, and stated
that simulators, emulators, and containers do not substitute for any of them.

**Amended by the maintainer (Samuel Ezenma, 2026-07-31):** simulator, emulator,
and pinned-container evidence satisfies conditions 2 and 3 for cutover.

What this changes and what it does not:

- It **does** accept that the generated hosts compile, launch, render native
  widgets, expose accessible names to each platform's real accessibility API,
  and drive bound state, because all of that is what was executed.
- It **does not** establish device-specific behavior: real GPU drivers, vendor
  accessibility services, thermal and memory pressure, and physical input
  remain unexercised. A defect that only appears on hardware would not have
  been caught by any run recorded here.
- [`SUPPORT_TIERS.md`](SUPPORT_TIERS.md) carries the same amendment, so a tier
  claim and this gate cannot drift apart.

## 1. Compatibility Evidence — Met

`node scripts/compat/differential.mjs`, 2026-07-31:

```
3/3 corpus projects match across implementations
```

Both implementations render in a real browser and their route graphs, semantic
DOM, and HTTP status are compared. The browser gates pass alongside it —
events, island expressions, offline service worker, the wasm ABI probe, and the
five-language wasm companion gate.

`changed` and `unsupported` rows are reported by `ty migrate check` with a
required action per finding. Against the archived migration fixture: 18 supported, 6
changed, 20 unsupported. Against this repository's migrated website on
2026-08-01: 128 supported, 1 changed, 1 unsupported. Cross-document navigation
is the changed finding; telemetry is the remaining unsupported product boundary recorded
in [`PARITY_LEDGER.md`](PARITY_LEDGER.md).

The migrated website's public command was re-run on 2026-08-01 after dynamic
route and post-bundle compatibility work:

```text
bun run test
  24 pass, 0 fail, 160 expectations
```

The same Rust release binary emitted its 11 web routes plus Android, iOS, and
macOS application bundles. Both `serve` and `preview` returned the concrete
dynamic `/docs/introduction` route rather than a 404.

## 2. Native Evidence Profiles — Met, with macOS and Windows Named

Executed 2026-07-31 on this machine:

| Target | Command | Result |
| --- | --- | --- |
| iOS | `ty build --target ios`, `simctl install`, `simctl launch` on iPhone 17 Pro / iOS 26.5 | Launched as pid 13540. Native large title, body, `UIButton`, bound output, `UITextField`, and disclosure rendered. Two taps moved the bound count `0 → 2`; typing `Ada Lovelace` produced 12 `state.input` events; `<x-chart>` rendered as an isolated `WKWebView` subtree showing "Chart fallback" while every sibling stayed native. The lifecycle log recorded `controller.created → route.opened → websurface.created → websurface.attached → controller.mounted → controller.active → state.increment → state.input → controller.suspended`. |
| Android | `./scripts/android/native-test.sh` on the `tachyon-gate` emulator (API 36, `google_apis`, arm64) | `PASS: Android native gate`. 5 accessible names on native widgets, `Increase count` as `Button` and `Your name` as `EditText`; accessibility-driven activation moved the bound output to `1`; lifecycle log complete. |
| Linux | `./scripts/linux/native-test.sh` inside the pinned `debian:trixie-slim` image | `PASS: Linux native gate`. The generated GTK4 host compiled under `-Werror`, launched under `Xvfb` and `dbus-run-session`, and 7 declared accessible names reached AT-SPI with correct platform roles; AT-SPI activation incremented the bound state. |
| macOS | `node scripts/phase4-macos-test.mjs` | **Blocked, not failed.** The probe cannot read the window until macOS Accessibility permission is granted to the process that runs it: `phase4 accessibility probe: macOS Accessibility permission is not enabled for this process`. Grant it in System Settings → Privacy & Security → Accessibility and re-run. |
| Windows | `scripts/windows/native-test.ps1` in [CI run 30714886154](https://github.com/d31ma/Tachyon/actions/runs/30714886154) | `PASS: Windows native gate`. The generated `PE32+` app compiled, launched, exposed its visible names through UI Automation and a real `Button` HWND, processed `BM_CLICK`, changed bound state `0 → 1`, recorded lifecycle, and closed. The managed UIA client's pane-only roles remain a named promotion gap. |

Additional migrated-website execution on 2026-08-01 built all 11 routes into
iOS, Android, and macOS artifacts. iOS and macOS launched and rendered the
homepage. On Android, real emulator taps navigated through `/docs` to the
dynamic `/docs/introduction` route and the generated host recorded no fatal
runtime exception. This evidence exercises same-origin WebSurface-to-native
route handoff that the earlier phase fixture did not contain.

## 3. Install, Upgrade, Rollback, Uninstall — Met

`./scripts/release/lifecycle-drill.sh`, 2026-07-31:

```
ok   the binary is bit-identical across builds
ok   every published file matches SHA256SUMS
ok   a modified artifact fails verification
ok   installed ty 26.33.01
ok   the installed tool built and published a project
ok   upgraded in place and still builds
ok   rolled back to ty 26.33.01 and still builds
ok   uninstall left nothing behind
PASS: release lifecycle drill
```

The drill could not run from a clean checkout before today: every gate script
in `scripts/` was committed without its executable bit, so the drill failed at
`build-artifact.sh: Permission denied`, and CI's `./scripts/linux/native-test.sh`
would have failed the same way. The bits are set now.

The same lifecycle for generated native application artifacts is covered by the
amended evidence standard: each platform's gate installs, launches, and drives
the generated application, and removes it with the emulator or container it ran
in.

## 4. Signed, Attested, Independently Verifiable — Open Until a Release Runs

**Independently verifiable: met.** Each artifact carries `SHA256SUMS` over every
published file and a `manifest.json` recording the release version, commit,
`SOURCE_DATE_EPOCH`, target, and toolchain. Builds are reproducible — proven
again today by the bit-identical rebuild above — so a third party can rebuild
from the recorded commit and compare digests without trusting the publisher.
`rust-ci.yml` also publishes a CycloneDX SBOM and builds with `cargo auditable`.

**Attested: the repository work is complete.** The release lifecycle job now
attests build provenance with `actions/attest-build-provenance` from the
workflow's own identity, so what remains is one real workflow run rather than a
maintainer key. Verification after that run:

```bash
gh attestation verify tachyon-<target>.tar.gz --repo <owner>/TACHYON
```

The stable workflow now gives every asset a keyless Sigstore signature bound
to the workflow identity, stages a non-public draft, verifies the checksum,
attestation, signature, raw executable, and installer on all five release
runners, and only then makes the release public. This repository work still
needs one real tag run before the condition is evidence-backed.

## 5. Threat-Model Findings — Met by Automated Qualification

An independent automated technical audit on 2026-08-01 found two high-severity
issues: descendant handler processes could escape lifecycle control, and Linux
WebSurfaces accepted arbitrary `file://` navigation. Both were remediated and
the technical re-review found no remaining critical or high blocker.

What exists instead, from [`PHASE_7_EVIDENCE.md`](PHASE_7_EVIDENCE.md):
7,197,692 fuzz executions across four trust boundaries under
`AddressSanitizer` with zero crashes, a clean sanitizer run, and five recovery
drills. [`SECURITY_REVIEW_PACKAGE.md`](SECURITY_REVIEW_PACKAGE.md) contains the
technical audit record and the automated qualification criteria.

The stable tag requires a clean exact-head security matrix and an explicit
owner and disposition for every critical or high finding. Independent human
review remains welcome but is not a release gate.

## 6. Stable Documentation — Met

`README.md`, `CONTEXT.md`, the architecture overview, support tiers, cutover
status, and the migrated website now describe the Rust implementation. Legacy
OpenAPI, telemetry, client-renderer, runtime-cache, and native-host claims were
removed from the stable README or named explicitly as compatibility decisions.
Commands, ports, output paths, Yon context composition, real-language Wasm
companions, native-first rendering, per-subtree fallback, security boundaries,
and support vocabulary were checked against the executable implementation and
its recorded evidence.

## What Happens Next

Complete the pull-request matrix, then run the release workflow from the
qualified immutable tag and verify its staged assets.
The macOS
Accessibility automation remains a named local evidence gap and must not be
represented as completed platform promotion work.

Per [`RELEASE_ENGINEERING.md`](RELEASE_ENGINEERING.md), `main` remains the
released implementation until that pull request lands, and history is never
rewritten.
