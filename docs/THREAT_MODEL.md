# Tachyon Rust Rewrite Threat Model

## Scope

This model covers project discovery, HTML and companion parsing, compiler
inputs and outputs, route manifests, handler processes, development and
production servers, generated web artifacts, native hosts, WebSurface
fallbacks, capability bridges, caches, installers, update channels, release
artifacts, and CI.

It is a living model. Every vertical slice updates concrete data flows,
trust boundaries, abuse cases, and validation evidence.

## Assets

- application source and generated artifacts;
- server secrets and environment configuration;
- user request and response data;
- filesystem and network capabilities;
- native platform credentials and secure storage;
- handler runtime integrity;
- build and release identities;
- signing keys, provenance, and update metadata;
- developer worktrees and caches.

## Trust Boundaries

1. Untrusted project files to project discovery.
2. HTML, styles, components, and companion source to compiler frontends.
3. Compiler stages to staging and output directories.
4. Network clients to Tachyon's HTTP edge.
5. Server runtime to supervised Yon handler processes.
6. Handler processes to environment, filesystem, and network resources.
7. View IR to web and native code generators.
8. Native UI to an isolated WebSurface subtree.
9. WebSurface content to the native capability bridge.
10. Generated platform projects to external mobile and desktop toolchains.
11. Installer and updater to release archives and the local filesystem.
12. CI jobs to third-party actions, registries, caches, and release identities.

## Primary Threats and Controls

| Threat | Required controls |
| --- | --- |
| Path traversal or output overwrite | project-relative canonical paths, symlink checks, staging directories, atomic publication, explicit output root |
| Compiler resource exhaustion | byte, node, depth, recursion, expansion, diagnostic, and time budgets |
| Template injection | structural parsing, typed bindings, context-aware escaping, no implicit eval |
| Malicious custom tags | resolve control tags at compile time; adapter allowlists; isolated local WebSurface fallback |
| Route-context confusion | deterministic contributors, duplicate-key error, stable manifest, no last-writer-wins |
| Shell or argument injection | direct process spawning, fixed argv boundaries, no shell interpolation |
| Handler denial of service | bounded frames, deadlines, cancellation, concurrency limits, process memory/CPU policy, forced termination |
| Handler protocol desynchronization | four-byte length framing, maximum frame, strict schema, request IDs, stderr separation |
| Environment secret disclosure | allowlisted environment, redacted diagnostics, secret-canary tests |
| SSRF and unrestricted egress | deny-by-default network capability, normalized origin allowlists, DNS/redirect revalidation, response limits |
| Native bridge escalation | explicit capability manifest, per-call validation, local content only, remote bridge always disabled |
| WebSurface origin confusion | isolated storage/origin, local bundle identity, no ambient navigation privilege |
| Cache poisoning | versioned cache keys include compiler, contract, target, config, and source digests; verified atomic writes |
| Malicious archive | bounded extraction, path and symlink validation, explicit manifest, digest verification |
| Supply-chain compromise | locked dependencies, advisory/license/source policy, pinned actions, minimal release permissions, SBOM, provenance, signatures |
| Compromised release job | protected environments, tag/version verification, native builders, attestations, post-upload verification |
| Diagnostic data leakage | allowlisted structured fields, bounded snippets, no secrets or full environment dumps |

## Security Invariants

- Invalid or ambiguous security configuration fails closed.
- Remote content receives no native bridge.
- Compiler and handler inputs are untrusted regardless of repository origin.
- A successful build never writes outside its declared output and cache roots.
- A successful handler response belongs to exactly one request ID.
- Cancellation and parent termination eventually reap every owned child.
- Capability absence means denial.
- No support claim is based on cross-compilation alone.

## Phase 0 Evidence

- `unsafe_code = "forbid"` at workspace scope.
- canonical contracts include rejected adversarial examples;
- dependency licenses, sources, and advisories are policy-gated;
- GitHub Actions are pinned to immutable commits;
- release permissions are least-privilege and job-scoped;
- public limitations are recorded in `docs/SUPPORT_TIERS.md`.

## Phase 1 Evidence

- Discovery rejects symlinks, path escapes, invalid route shapes, collisions,
  dynamic segments, and later-phase companions.
- HTML input is limited to 1 MiB, UTF-8, and NUL-free content; scripts, inline
  event handlers, compiler control tags, and unresolved bare components fail
  closed.
- Builds write to a sibling staging directory and replace published output
  only after every route and the manifest succeed; failures retain the previous
  output.
- The development server builds before binding, defaults to loopback, requires
  explicit opt-in for non-loopback addresses, serves only generated files, and
  emits CSP, frame, MIME-sniffing, referrer, and cache controls.
- Real-binary tests cover missing routes and a raw traversal request in
  addition to successful GET and HEAD behavior.

## Phase 2 Evidence

- Handler sources are project-contained, regular, non-symlinked
  `server/routes/**/yon.js` or `yon.py` files with UTF-8, NUL, and 1 MiB
  validation.
- Runtime programs are spawned directly with fixed argument boundaries and no
  shell. Embedded adapters are selected from the discovered source language;
  there is no extension fallback.
- The child environment is cleared, then rebuilt from a small platform runtime
  baseline and explicitly named allowlist entries. Tests prove a host value is
  absent by default and present only when allowed.
- Protocol stdout accepts exactly one UTF-8 JSON frame with a four-byte
  big-endian length and 16 MiB maximum. Request IDs, kinds, versions, fields,
  bodies, headers, statuses, and adapter error shapes are validated.
- stderr is drained concurrently and capped at 64 KiB. Adapter console output,
  exceptions, and syntax/load errors produce bounded public messages without
  tracebacks or environment values.
- One deadline covers semaphore admission, spawn, execution, response, and
  exit. Cancellation sends a typed frame before a 100 ms default grace period;
  timeout and cancellation kill and reap uncooperative children.
- One process serves one request. Tests prove crash isolation and successful
  recovery through a fresh child, plus forced termination of an infinite loop.
- Real-process adversarial tests cover trailing-output smuggling, oversized
  output, mismatched request IDs, stderr floods, missing methods and classes,
  syntax errors, exceptions, non-serializable results, concurrency queue
  expiry, Unicode paths and payloads, and absent runtimes.

Phase 2 is process supervision, not a security sandbox. Application handlers
retain the invoking developer account's ambient filesystem and network access,
and the operating system does not yet enforce CPU or memory quotas. Production
HTTP dispatch, egress capabilities, application dependency imports, process
pooling, and streaming are out of scope; no documentation may imply those
controls exist.

## Phase 3 Evidence

- Template expressions use a bounded JSON-only parser with no calls,
  assignment, construction, prototype access, raw HTML, shell, or `eval`.
- Text and attributes are escaped by output context. Component templates see
  only evaluated properties; slotted children retain their parent scope.
- JavaScript and Python view contexts are returned through the supervised
  protocol, bounded to 1 MiB, limited to 1,024 exports and depth 32, merged in
  canonical order, and rejected on any duplicate or reserved built-in.
- View IR is emitted before context evaluation. View IR, source maps,
  manifests, diagnostics, and build state contain export names and digests,
  never context or island values.
- Island props are explicit public client input. Modules use compiler-generated
  same-origin paths, activate against useful SSR DOM, mark bounded failures,
  and never expose a native or server capability bridge.
- Incremental state is disposable and untrusted. Reuse requires a supported
  state version, contained regular paths, and matching SHA-256 for every route
  artifact; handler-backed routes are never reused.
- Source, expression, template depth, component count, context, iteration,
  expanded-node, rendered-output, diagnostic, process, and protocol limits are
  enforced by tests.
- Real Chromium evidence covers automatic activation, interaction replay,
  failure marking, static `never` islands, and SSR preservation.

## Phase 4 Evidence

- Native planning consumes resolved Phase 3 HTML, uses a fixed adapter
  allowlist, assigns deterministic identifiers, and emits the smallest
  unsupported custom-element or island subtree as a WebSurface.
- `tachyon.json` is UTF-8, regular, non-symlinked, limited to 64 KiB, strict
  about unknown keys, and validated before native publication.
- Controller names, scalar values, state entries, action verbs, numeric
  increments, node depth/count, surface count, fallback bytes, Swift source,
  tool output, and lifecycle log fields are bounded. Invalid declarations fail
  before Swift compilation.
- Local WebSurfaces receive generated deny-by-default CSP, bundle-contained
  navigation, a non-persistent store, and no script message handler. Remote
  WebSurfaces require a strict HTTPS host, disable content JavaScript, restrict
  redirects to that host, and always use `bridge: none`.
- Capability Manifest v1 declares `default_policy: deny`, no capabilities, and
  `remote_content_bridge: false`. Artifact Manifest v1 records toolchains,
  contract versions, every output path, size, and SHA-256 digest.
- Native builds use contained staging directories, reject generated symlinks,
  and atomically replace only `dist/macos`; a failed build preserves the prior
  complete application.
- Real macOS evidence proves named Accessibility controls, keyboard input,
  action/disclosure state, lifecycle termination, isolated WebKit content, and
  parity with required mobile-web roles and names.

## Phase 5 Evidence

- Every generated native host runs its `WebSurface` with an ephemeral,
  non-persistent data store, enables scripting only for local-bundle content,
  and cancels any navigation outside the bundle root or the declared HTTPS
  host.
- No generated host exposes a script message handler, a JavaScript interface,
  or any other bridge to web content. Unit tests assert the absence of each
  platform's bridge API by name.
- Capability Manifest v1 is emitted deny-by-default for every target, with
  `remote_content_bridge` false.
- Every platform toolchain is invoked directly, never through a shell.
- Generated output containing a symlink is rejected before publication.

## Phase 7 Evidence

- Four fuzz targets cover the trust boundaries named below. 7,197,692 total
  executions under `AddressSanitizer` produced zero crashes, timeouts, or
  out-of-memory conditions. Details in `PHASE_7_EVIDENCE.md`.
  - `handler_frame` decodes frames written by an untrusted child process —
    4,482,194 executions.
  - `html_frontend` and `template_frontend` parse application source.
  - `native_planner` lowers resolved HTML and asserts that all five native
    targets agree on validity, so platform selection cannot widen what is
    accepted.
- The library suite passes under `AddressSanitizer`. `LeakSanitizer` and
  `ThreadSanitizer` run in CI on Linux.
- Recovery drills assert that publication is atomic: an interrupted, failed,
  or concurrent build never leaves partial or torn output, and a corrupted
  incremental cache is detected rather than trusted.
- A CycloneDX bill of materials is generated, and the release binary embeds
  its dependency list through `cargo auditable`.
- The `fuzzing` feature exposes crate-private parsers to fuzz targets only. It
  carries no compatibility guarantee and must never be enabled in a released
  artifact.

## Remaining Security Gates

- **An independent security assessment before stable cutover. This is a human
  deliverable and remains open.** It cannot be satisfied by any automation in
  this repository, and no target may reach `supported` while it is open.
- enforce handler filesystem, network, CPU, and memory capabilities before
  production routing;
- run live remote-origin/DNS/redirect `WebSurface` adversarial tests;
- run secret-canary logging tests;
- complete platform sandbox and production signing reviews;
- run scheduled long-form fuzz campaigns with a corpus carried between runs.
