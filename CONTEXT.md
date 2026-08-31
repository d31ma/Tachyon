# Tachyon Product Context

## Purpose

Tachyon is a polyglot, file-system-routed full-stack framework. Application
developers write standards-based HTML for views while Tachyon owns discovery,
compilation, server lifecycle, handler supervision, web output, and native
application generation.

The Rust rewrite is a greenfield implementation. The in-tree JavaScript
framework was removed after cutover. Compatibility is measured against the
immutable v26.30.04 release binary and neutral fixtures in `corpus/`, never by
importing private legacy implementation code.

## Domain Language

- **Tac** is the client-side view and interaction layer. A `tac.html` document
  may have colocated `tac.*` controller and style companions.
- **Yon** is the server-side REST endpoint and handler layer. A `yon.*` source
  exports or implements HTTP method handlers.
- **View source** is `tac.html`. It contains HTML and may use structural
  control tags interpreted in the client.
- **Route graph** is the deterministic, immutable result of file-system
  discovery before a build or server generation starts.
- **Control tag** is `if`, `else`, `for`, or `loop`. It is compiler syntax,
  interpreted by the Tac browser renderer, never an unknown HTML element sent
  to a browser or native renderer.
- **Tac component** is a framework component resolved through the project
  component graph.
- **Web component** is a standards-based custom element. It may render on the
  web or use a registered native adapter.
- **View IR** is the canonical platform-neutral representation produced after
  parsing, validation, component resolution, and control-tag lowering.
- **Native adapter** maps a View IR element or component to a native platform
  primitive.
- **WebSurface** is an isolated local web-rendered subtree used when a native
  adapter is unavailable. It is not an application-wide render mode.
- **Capability** is an explicit permission to access a sensitive host or
  platform resource. The default policy is deny.
- **Buildable**, **simulator-tested**, **native-tested**, **preview**, and
  **supported** are distinct evidence levels defined in
  `docs/SUPPORT_TIERS.md`.

## Relationships

1. Project discovery produces a route graph.
2. The HTML frontend parses `tac.html` into source-aware syntax.
3. Validation resolves control tags, bindings, and components.
4. Lowering emits deterministic View IR.
5. Web code generation consumes View IR and emits web artifacts.
6. Native planning consumes View IR and chooses a native adapter or a
   WebSurface boundary for each subtree.
7. The server invokes Yon handlers through a versioned protocol and an
   environment-selected supervised isolation backend, then passes their
   validated HTTP responses through unchanged.
8. Packaging records inputs, toolchains, contract versions, outputs, and
   digests in an artifact manifest.

## Non-Negotiable Invariants

- HTML is the only view language exposed to application developers.
- Tac is exclusively client-rendered; no Tac expression, branch, loop, or
  component subtree is rendered on the server.
- Yon is REST-only: it never renders a template or executes during a build.
- HTML returned by Yon requires an explicit `Content-Type: text/html` response
  and is transported without interpolation or framework markup injection.
- Control tags never reach a browser or native renderer as unknown elements.
- Source discovery, route ordering, diagnostics, IR, and artifacts are
  deterministic for identical inputs.
- A route may contain multiple same-level `yon.*` handlers in deterministic
  source order.
- Unsupported native content falls back at the smallest safe subtree.
- Supported siblings remain native when one subtree falls back.
- Remote web content never receives a native capability bridge.
- Capabilities are deny-by-default and scoped by resource and target.
- Handler input, output, runtime, environment, concurrency, and lifetime are
  bounded.
- Handler processes are never invoked through a shell.
- Public schemas and diagnostic codes are independently versioned contracts.
- Cross-compilation proves buildability only.
- No platform or feature is called supported without its published evidence.

## Explicit Phase 0 Decisions

- The new core is Rust and is organized as a modular monolith.
- Public machine contracts use JSON Schema Draft 2020-12.
- Handler envelopes use a length-prefixed JSON process protocol.
- Native fallback is per subtree and local-bundle-first.
- The current CLI and fixture behavior form the compatibility baseline.

## Current Implementation Boundary

Engineering phases 0–7 are implemented. The current Rust data path:

1. discovers static and dynamic `tac.html` routes and `yon.*` REST handlers;
2. parses bounded HTML, expressions, control tags, components, slots, events,
   styles, and browser component mount schedules;
3. validates mandatory layer stereotypes and supervises JavaScript, TypeScript,
   Python, Java, C#, Kotlin, PHP, and Rust Yon handlers only when an HTTP
   request, middleware phase, worker tick, or explicit invocation reaches them;
4. emits deterministic Tac client render plans, View IR, source maps, client
   modules, shared assets, a service worker, and Route Manifest v1;
5. dispatches exact and dynamic HTTP routes, before/after middleware, workers,
   and durable SSE topic logs from a loopback-safe development server;
6. stages the same client-rendered bundle for macOS, iOS, Android, Linux, and
   Windows web-view hosts, selecting target-native page companions per route;
7. compiles the generated host and selected companions with platform toolchains and
   publishes capability and artifact manifests atomically; and
8. verifies compatibility, recovery, performance, supply-chain, and release
   lifecycle behavior through executable gates.

The development server additionally publishes Hot Update Protocol v1. Tac's
production client renderer owns initial and reactive structure. Development
updates must target that render-plan boundary or reload safely; they may not
reintroduce SSR-island ownership. See ADR 0015.

The implementation is not yet a supported release. Buildable,
simulator-tested, native-tested, preview, and supported remain distinct
evidence levels in `docs/SUPPORT_TIERS.md`.

## Deliberately Deferred Decisions

These are not hidden gaps in a completed compatibility claim. Each remains a
named product or viability decision:

- a first-party, production-qualified Firecracker control program and host
  profile; the environment-selected transport boundary is implemented, but it
  does not by itself prove OS-level isolation;
- a stable third-party compiler or native-adapter plugin ABI;
- production signing identities and store-specific publication policy;
- whether out-of-scope legacy OpenAPI and telemetry products return in a
  separately owned form.

No deferred decision may be smuggled into production code without an ADR or RFC
when its compatibility or security consequences become concrete.

## Current Non-Goals

- reproducing the legacy unbounded client renderer or its mutable global
  component registry;
- implicit imports from application dependency graphs inside handler adapters;
- pooling or reusing handler processes;
- giving remote or local WebSurfaces a native capability bridge;
- treating successful cross-compilation as native runtime evidence;
- publishing a production release before the cutover gate is met.
