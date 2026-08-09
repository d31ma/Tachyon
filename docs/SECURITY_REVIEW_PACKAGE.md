# Security Qualification Package

This document defines the automated security qualification required for a
stable release and remains a useful brief for optional independent review.
Exact-head CI, the technical audit record, and explicit disposition of every
critical or high finding close the security gate.

## 1. What to Review

The Rust implementation under `crates/`, the generated native hosts under
`crates/tachyon-core/src/native/`, and the public contracts under `api/`.

The archived JavaScript release is **not** in scope and is not present in this
branch. CI downloads its checksum-verified v26.30.04 executable only for the
behavioral compatibility differential.

## 2. Start Here

| Document | What it gives you |
| --- | --- |
| [`CONTEXT.md`](../CONTEXT.md) | Domain language, invariants, and non-goals |
| [`THREAT_MODEL.md`](THREAT_MODEL.md) | Assets, trust boundaries, threats, controls, and per-phase evidence |
| [`architecture/OVERVIEW.md`](architecture/OVERVIEW.md) | How the pieces fit |
| [`adr/`](adr/) | Six accepted decisions and their rejected alternatives |
| [`PHASE_7_EVIDENCE.md`](PHASE_7_EVIDENCE.md) | Fuzzing, sanitizer, drill, soak, and budget results |
| [`SUPPORT_TIERS.md`](SUPPORT_TIERS.md) | What is and is not claimed for each target |

## 3. Trust Boundaries to Examine

| Boundary | Where | Existing assurance |
| --- | --- | --- |
| Application HTML into the compiler | `html.rs`, `template/` | Bounded parser, fuzzed |
| Untrusted child-process output | `handler/frame.rs` | Length-prefixed protocol, fuzzed at 4.4M executions |
| Handler process lifetime and environment | `handler/process.rs` | Direct spawn, never a shell; deny-by-default environment; deadlines, cancellation, reaping |
| Generated output path handling | `compiler.rs`, `native/host.rs` | Project containment, symlink rejection, atomic publication |
| Web content inside a native host | `native/{macos,ios,linux,windows,android}.rs` | Ephemeral store, navigation policy, no bridge of any kind |
| Native capability surface | `CapabilityManifest` | Deny-by-default, `remote_content_bridge` false |
| Dependency supply chain | `deny.toml`, CI | Advisories, bans, licenses, sources; pinned actions; SBOM; auditable binary |

## 4. Questions We Want Answered

1. Can any input reach a native `WebSurface` that obtains a capability, a
   bridge, or navigation outside its declared origin?
2. Can a handler child process influence the supervisor beyond its declared
   response — through frame framing, stream interleaving, resource exhaustion,
   or process lifetime?
3. Can a crafted project escape its own directory during discovery, staging,
   or publication, on any supported platform including Windows path semantics?
4. Are the bounds in the specifications — sizes, depths, counts, deadlines —
   sufficient to prevent denial of service on a build machine?
5. Does any generated host source permit injection from application-authored
   strings? Escaping is centralized in `native/host.rs`; is it complete for
   Swift, Java, C, XML, and property-list contexts?
6. Are the deliberate reductions in `PHASE_5_SPEC.md` §6 and the deferred
   decisions in `CONTEXT.md` acceptable at the tiers claimed for them?

## 5. Known Gaps, Declared Up Front

These are recorded so a reviewer does not spend time rediscovering them.

- OS-level filesystem, network, CPU, and memory capability enforcement for
  handler children is not implemented. The child runs with the developer
  account's ambient access.
- Production HTTP dispatch and middleware exist and run application handlers
  with the developer account's ambient filesystem and network access. Their
  public error responses must not expose process diagnostics.
- Native signing is ad-hoc or debug-key only on every platform.
- Windows exposes no accessible name distinct from a control's visible text,
  and embeds no web view.
- Long-form fuzz campaigns with a persistent corpus have not been run.
- The curl/PowerShell bootstrap installers fail closed on release checksums,
  while Sigstore bundles and GitHub attestations are verified by the release
  workflow. A standalone installer cannot authenticate those proofs without
  an already-trusted `cosign` or GitHub CLI verifier; an authenticated
  bootstrap path remains a medium-severity supply-chain improvement.

## 6. Qualification Record

A completed qualification must state the date, exact commit, scope covered,
and every finding with a severity, owner, and disposition. Independent human
review may be appended here but is optional.

### Automated technical pre-review — 2026-08-01

- Reviewer: Codex Security Engineer role #7 (automated qualification).
- Candidate: base commit `2d2fbaf9846d641c86ce4258656562158f08defa`
  plus the current release diff.
- Scope: handler supervision, native WebSurfaces, server error boundaries,
  installers, release/CI workflows, dependencies, and security documentation.
- Initial findings: two high-severity issues in descendant process lifecycle
  ownership and Linux local-resource navigation; five medium findings in
  public diagnostic redaction, remote-origin port comparison, Unix upgrade
  atomicity, installer-authentication bootstrap, and review-package accuracy.
- Re-review disposition: both high findings and four medium findings are
  closed with code and regression evidence. Installer-authentication bootstrap
  remains a documented medium improvement and needs a named owner; it is not a
  critical/high technical release blocker.

This record satisfies the security gate when the final exact-head enterprise
qualification passes and no critical or high finding is unowned or unresolved.
Optional independent review does not block release.
