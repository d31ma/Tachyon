# Tachyon Rust Rewrite Threat Model

Toolchains and project hooks are untrusted child process trees. Tachyon bounds
their wall time and retained output and terminates the complete POSIX process
group or Windows Job Object on completion, cancellation, or timeout. Topic log
subscriptions accept only bounded slugs, reject symlink/non-regular or
identity-raced inputs, cap cumulative reads at 16 MiB, and evict lagging or
over-capacity subscribers without exposing filesystem details. Topic errors
use a named `topic-error` SSE event with canonical sanitized JSON; they never
reuse the handler-stream `error`/`TY2107` contract or include raw log content.

## Scope

This model covers project discovery, HTML and companion parsing, compiler
inputs and outputs, route manifests, handler processes, development and
production servers, generated web artifacts, native web-view hosts,
capability bridges, caches, installers, update channels, release
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
5. Server runtime to the selected Yon isolation backend.
6. Handler processes or a trusted Firecracker control program to environment,
   filesystem, network, microVM, and host resources.
7. View IR to web and native code generators.
8. Staged application bundle to the platform's local-origin web view.
9. Main-frame local content to the route-scoped native capability bridge.
10. Generated platform projects to external mobile and desktop toolchains.
11. Installer and updater to release archives and the local filesystem.
12. CI jobs to third-party actions, registries, caches, and release identities.
13. Untrusted development source and diagnostics to the hot-update browser
    client.

## Primary Threats and Controls

| Threat | Required controls |
| --- | --- |
| Path traversal or output overwrite | project-relative canonical paths, symlink checks, staging directories, atomic publication, explicit output root |
| Compiler resource exhaustion | byte, node, depth, recursion, expansion, diagnostic, and time budgets |
| Template injection | structural parsing, typed bindings, context-aware escaping, no implicit eval |
| Malicious custom tags | bounded client render plans; no eval; compiler-owned AST; restrictive CSP |
| Route-context confusion | deterministic contributors, duplicate-key error, stable manifest, no last-writer-wins |
| Shell or argument injection | direct process spawning, fixed argv boundaries, no shell interpolation |
| Handler denial of service | bounded frames, deadlines, cancellation, concurrency limits, process memory/CPU policy, forced termination |
| Handler protocol desynchronization | four-byte length framing, maximum frame, strict schema, request IDs, stderr separation |
| Isolation downgrade by application source | parent-environment-only selection; no project or request override; invalid and partial policy fails closed |
| False Firecracker assurance | absolute regular non-symlinked and non-group/world-writable control-program path; bounded policy arguments; hardware-isolation claims require separate driver and host evidence |
| Environment secret disclosure | allowlisted environment, redacted diagnostics, secret-canary tests |
| SSRF and unrestricted egress | deny-by-default network capability, normalized origin allowlists, DNS/redirect revalidation, response limits |
| Native bridge escalation | declared capabilities; bounded per-route registry; operation/member/payload checks; trusted main-frame local origin only |
| Native origin confusion | canonical bundle resource mapping; frame and navigation guards; no remote bridge; no arbitrary file URLs |
| Cache poisoning or path replacement | versioned cache keys include compiler, contract, target, config, and source digests; non-following directory capabilities; identity-checked cleanup; verified atomic writes; private runtime copies |
| Malicious archive | bounded extraction, path and symlink validation, explicit manifest, digest verification |
| Supply-chain compromise | locked dependencies, advisory/license/source policy, pinned actions, minimal release permissions, SBOM, provenance, signatures |
| Compromised release job | protected environments, tag/version verification, native builders, attestations, post-upload verification |
| Diagnostic data leakage | allowlisted structured fields, bounded snippets, no secrets or full environment dumps |
| Development hot-update injection or stale execution | versioned typed messages, JSON serialization, text-only diagnostic rendering, same-origin generated modules, compiler-owned island boundaries, last-good output, and reload on ambiguity |

## Reconciled request and browser boundaries

The following describes implemented controls. These documentation updates
are not a new assessment or signoff; the separate cybersecurity review for
this reconciliation was waived by the user.

- CHEX validates immutable captured schemas against private request files;
  request bodies and authorization headers never enter argv or public errors.
  Admission and execution share a deadline, concurrency is capped, and invalid
  schema/tool startup fails before publication or socket readiness.
- Browser response persistence is opt-in for same-origin credential-omitted
  GET/HEAD requests. Authorization, cookie-bearing requests, private/no-store
  responses, oversized bodies, and unsupported Vary responses bypass storage.
  `Range`/`If-Range` requests bypass both cache lookup and persistence, including
  offline fallback. `206` responses and responses with `Content-Range` are
  excluded on both writes and reads of existing cache records.
  Browser storage is not a secret store; $/$$ fields must not contain secrets.
- Pub/sub and retained messages are bounded, detached on abort/unmount/HMR,
  and do not acquire native authority merely by sharing a topic name.
- Native companions are application-trusted code with host privileges, not
  sandboxed plugins. Origin and route checks constrain who can invoke them;
  they cannot make malicious compiled application code safe.
- Android pins AndroidX WebKit `1.14.0` and uses its frame-aware asynchronous
  message listener and document-bound reply proxy. There is no legacy
  JavaScript-interface fallback; unsupported WebView runtimes report bridge
  unavailability. Document calls are capped at 128 with ten-second deadlines;
  one worker admits at most 128 queued requests. Navigation retires pending
  replies and stale queued work; destruction shuts down the worker.
- Apple hosts and Swift companions share Foundation JSON decoding and
  canonical request serialization instead of the handwritten value scanner.
  A byte-level guard enforces 64 KiB and 64-level limits and rejects duplicate
  root keys before decoding. Dispatch uses the same canonical payload that
  the host validated.
- Native field snapshots use per-field revisions so delayed replies cannot
  overwrite newer edits. Native-call timeout limits browser waiting, not
  preemption or rollback of already-running companion code. Companions must
  remain responsive; recovery from hung native work may require relaunch.
- Historical native and WASM evidence later in this document is superseded
  by ADRs 0018/0019 and must be requalified for this release.

## Invariants enforced across both architectures

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

- Discovery opens one project capability, rejects symlinks at every traversed
  component, and builds routes and layer diagnostics from bytes read through
  that retained handle. Ambient root replacements cannot redirect views,
  routes, components, shared assets, build/native configuration, or any of the
  five layer roots during the pass. Web and native compilers consume the same
  owned snapshot; native compilation does not perform a second discovery. The
  development server likewise consumes one Project for its initial build,
  routes, selected root middleware, captured worker schedules, and worker
  HandlerSources. A root swap cannot mix those startup inputs. The server owns
  the watcher and worker task set, cancels them with HTTP shutdown, boundedly
  awaits settlement, and aborts any remainder on drop; no worker or rebuild
  task is detached. Handler-stream bridges, hot-update SSE, and topic SSE are
  tracked under the same cancellation token. Cancellation precedes Axum's
  graceful connection wait and one global deadline bounds HTTP and task
  settlement. Its final slice is reserved for aborting and join-draining both
  task registries, preventing an infinite response from retaining a handler
  child or blocking watcher/worker cleanup. Completed producer records are
periodically reaped before shutdown, bounding registry retention by live
work rather than historical request count. Worker schedule changes take effect on
server restart, not during a web-only hot rebuild.
An infinite handler stream is signalled and then fail-fast dropped through its
supervised process-group guard; shutdown never polls cancellation-uncooperative
producer work again. Task-registry draining yields at a bounded cadence so an
immediately-ready backlog cannot starve the hard deadline.
- HTML input is limited to 1 MiB, UTF-8, and NUL-free content; scripts, inline
  event handlers, compiler control tags, and unresolved bare components fail
  closed.
- Builds write to a sibling staging directory and replace published output
  only after every route and the manifest succeed; failures retain the previous
  output.
- The development server builds before binding, defaults to loopback, requires
  explicit opt-in for non-loopback addresses, serves only generated files, and
  emits CSP, frame, MIME-sniffing, referrer, and cache controls. Each watched
  rebuild performs a fresh discovery and passes that Project to the compiler;
  shutdown cancels and settles the owned watcher before returning.
- Before the build or `--no-bundle` output check and before socket binding, the
  development server runs deadline- and output-bounded synthetic capability
  probes for the deduplicated runtime requirements of routes, root middleware,
  and configured workers. Static-only projects probe nothing. Firecracker
  readiness fails closed for unsupported discovered languages or an invalid
  driver; it does not claim that a remote microVM pool is healthy. The C# probe
  builds framework-owned source in a private temporary project with no package
  sources, proving SDK/build capability instead of trusting a runtime list.
- Real-binary tests cover missing routes and a raw traversal request in
  addition to successful GET and HEAD behavior.

## Phase 2 Evidence

- Handler sources are project-contained, regular, non-symlinked files in the
  eight owned Yon languages, with UTF-8, NUL, and 1 MiB validation. Sources in
  the five server layer roots must declare the matching stereotype and class
  suffix before runtime selection.
- The validated bytes are copied into an owned project-shaped source snapshot.
  Process adapters and the JavaScript/Python Firecracker source contract use
  that immutable copy; the authored absolute path remains diagnostic metadata
  and is never reopened as the handler program or working directory.
  Standalone handler discovery captures the complete bounded, non-following
  `server/**` tree so relative imports and relay programs resolve only from the
  owned snapshot.
- Runtime programs are spawned directly with fixed argument boundaries and no
  shell. Embedded adapters are selected from the discovered source language;
  arbitrary interpreter registration, shebang execution, executable-handler
  fallback, and class-name inference are rejected.
- A missing runtime is `TY2112` both during startup preflight and if it
  disappears in the later spawn race. Terminal and JSON diagnostics identify
  only the logical runtime or the relevant configuration variable. Structured
  failure events add only the runtime family and `not_found` category; raw
  override paths, OS errors, environment values, authored source, request
  bodies, and child output are excluded.
- Programs outside the owned language set cross only an explicit `@Delegate`
  plus `@Relay` boundary. Delegate stdout and stderr are drained concurrently
  under fixed bounds, public failures are redacted, and the supervisor retains
  the absolute deadline and complete process-group cleanup boundary.
- The child environment is cleared, then rebuilt from a small platform runtime
  baseline and explicitly named allowlist entries. Tests prove a host value is
  absent by default and present only when allowed.
- Protocol stdout accepts one UTF-8 JSON response frame with a four-byte
  big-endian length and 16 MiB maximum, or a declared stream of at most 100,000
  events, 256 KiB per event, and 64 MiB total. Request IDs, kinds, versions,
  fields, bodies, headers, statuses, and adapter error shapes are validated.
- stderr is drained concurrently and capped at 64 KiB. Adapter console output,
  exceptions, and syntax/load errors produce bounded public messages without
  tracebacks or environment values.
- One deadline covers semaphore admission, spawn, execution, response, pipe
  settlement, and exit. Each invocation owns a POSIX process group or Windows
  kill-on-close Job Object. Cancellation sends a typed frame before a 100 ms
  default grace period; success, timeout, and cancellation terminate and
  boundedly reap the complete process tree.
- One process serves one request. Tests prove crash isolation and successful
  recovery through a fresh child, plus forced termination of an infinite loop.
- Real-process adversarial tests cover trailing-output smuggling, oversized
  output, mismatched request IDs, stderr floods, missing methods and classes,
  syntax errors, exceptions, non-serializable results, concurrency queue
  expiry, Unicode paths and payloads, absent runtimes, and descendants that
  inherit protocol pipes. HTTP dispatch returns only a request reference for
  supervisor failures; a secret-canary test proves process diagnostics do not
  cross the network boundary.
- Stream failure sends one terminal, sanitized SSE `error` event containing a
  stable code and request ID, whether failure occurs before the first value or
  after partial delivery. Subscriber disconnect and deadline expiry terminate
  and reap the complete process group.
- Compiled handler artifacts are published atomically into a capability-confined
  `.tachyon/handlers` cache whose recursively accounted inactive entries are
  pruned to 256 entries and 512 MiB. Cache hits are copied into private runtime
  workspaces before execution. Lock recovery verifies process liveness,
  identity, and ownership token; a ten-minute lease bounds PID reuse.

Process mode is supervision, not a security sandbox. Application handlers
retain the invoking developer account's ambient filesystem and network access,
and the operating system does not enforce CPU or memory quotas there.
Application dependency imports and process pooling remain out of scope; no
documentation may imply those controls exist.

## Environment-Selected Isolation Evidence

- `process` remains the compatibility default. `firecracker` requires a
  complete environment policy and a directly executed absolute control-program
  path; invalid mode, path, pool, CPU, memory, or egress fails as `TY2010`.
- The control program receives a length-prefixed Handler Protocol v1 request,
  bounded non-secret policy arguments, a cleared environment, and the existing
  deadline/cancellation supervision.
- Only JavaScript and Python source reach the current driver contract.
  TypeScript and the prepared direct-language paths fail with `TY2010` before
  driver spawn; private runtime-workspace paths are not treated as transferable
  artifacts.
- Egress is deny-only. An allowlist cannot be requested until origin, DNS,
  redirect, and response-limit enforcement has its own acceptance evidence.
- This boundary does not qualify a Firecracker deployment. The control program
  and Linux host remain trusted components and must separately prove jailer,
  cgroup, namespace, seccomp, image, snapshot, credential, and network policy.

## Phase 3 Evidence

- Template expressions use a bounded JSON-only parser with no calls,
  assignment, construction, prototype access, raw HTML, shell, or `eval`.
- Text and attributes are escaped by output context. Component templates see
  only evaluated properties; slotted children retain their parent scope.
- Yon handlers never execute during builds and never contribute view context.
  Route Manifest v1 retains only a deprecated empty context member for wire
  compatibility; it contains no values or executable declaration.
- View IR, source maps, manifests, diagnostics, and build state contain no
  handler response or server-derived view values.
- Component props are explicit public client input. Modules use
  compiler-generated same-origin paths, mount against browser-created Tac DOM,
  mark bounded failures, and never expose a native or server capability bridge.
- Incremental state is disposable and untrusted. Reuse requires a supported
  state version, contained regular paths, and matching SHA-256 for every route
  artifact; handler-backed routes are never reused.
- Source, expression, template depth, component count, iteration,
  expanded-node, rendered-output, diagnostic, process, and protocol limits are
  enforced by tests.
- Real Chromium evidence covers client-owned rendering, automatic activation,
  interaction replay, bounded failure marking, and static `never` components.

## Phase 4 Evidence

- Native planning expands validated Tac component declarations without server
  rendering, uses a fixed adapter
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
- Linux local WebSurfaces use a private resource scheme rather than `file://`;
  decoded paths must resolve as regular, non-symlinked files beneath the
  initiating surface or generated WebBundle root. Remote surfaces on every
  platform compare HTTPS scheme, host, and effective port.
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

## Semantic Hot-Update Evidence

- Hot Update Protocol v1 permits only `css`, `island`, `reload`, and
  `diagnostics`; it cannot carry source text or an instruction to evaluate
  code.
- The development endpoint is disabled with `--no-watch`, is same-origin, uses
  a bounded broadcast queue, disables proxy buffering, and widens missed or
  lagged events to a full reload.
- The browser writes diagnostics with `textContent`, retains the last-good DOM
  on compiler failure, and reloads when an island boundary count or stable
  identity changes.
- Island replacement imports only compiler-generated same-origin module URLs.
  The old instance receives an abort signal and a disposal callback before its
  boundary is replaced. State transfer uses `structuredClone` on either the
  explicit `hotState()` result or the default enumerable non-function fields;
  a failed default clone retains an empty object. No JSON or depth bound is
  claimed.
- Browser-owned DOM state transfer snapshots at most the first 2,048 elements
  with an `id`. It preserves input and textarea values, input checked state,
  `<details>` disclosure, nonzero scroll offsets, and the focused element by
  id. It does not preserve text selection or selected `<option>` state, and it
  cannot transfer markup, attributes, listeners, arbitrary DOM properties, or
  contenteditable HTML.
- A real Chromium gate exercises CSS retention, island replacement, disposal,
  component and native DOM state retention, failed-build diagnostics,
  recovery, and structural reload without browser console errors.

## Remaining Security Gates

- Automated security gates remain mandatory for every release. An independent
  human assessment is recommended for major trust-boundary changes but is not
  a publication prerequisite; any accepted residual risk must be recorded in
  the release review package.
- enforce handler filesystem, network, CPU, and memory capabilities before
  production routing;
- run live remote-origin/DNS/redirect `WebSurface` adversarial tests;
- run secret-canary logging tests;
- complete platform sandbox and production signing reviews;
- run scheduled long-form fuzz campaigns with a corpus carried between runs.
