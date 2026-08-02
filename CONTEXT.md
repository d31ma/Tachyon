# Tachyon Product Context

## Purpose

Tachyon is a polyglot, file-system-routed full-stack framework. Application
developers write standards-based HTML for views while Tachyon owns discovery,
compilation, server lifecycle, handler supervision, web output, and native
application generation.

The Rust rewrite is a greenfield implementation and the only framework
implementation in this branch. Compatibility is measured against the
checksum-verified v26.30.04 release executable and neutral migration fixtures,
never against copied private implementation code.

## Domain Language

- **Tac** is the client-side view and interaction layer. A `tac.html` document
  may have colocated `tac.*` controller and style companions.
- **Yon** is the server-side route and handler layer. A `yon.html` document may
  have one or more colocated `yon.*` handlers.
- **View source** is either `tac.html` or `yon.html`. Both contain HTML and use
  the same structural control tags.
- **Route graph** is the deterministic, immutable result of file-system
  discovery before a build or server generation starts.
- **Route context** is the set of values available to one `yon.html` render.
  Static handler fields and the selected method's returned object contribute
  values.
- **Control tag** is `if`, `else`, `for`, or `loop`. It is compiler syntax,
  never an HTML element sent to a browser or native renderer.
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
2. An HTML frontend parses `tac.html` and `yon.html` into source-aware syntax.
3. Validation resolves control tags, bindings, and components.
4. Lowering emits deterministic View IR.
5. Web code generation consumes View IR and emits web artifacts.
6. Native planning consumes View IR and chooses a native adapter or a
   WebSurface boundary for each subtree.
7. The server invokes Yon handlers through a versioned, supervised process
   protocol and merges their permitted contributions into route context.
8. Packaging records inputs, toolchains, contract versions, outputs, and
   digests in an artifact manifest.

## Non-Negotiable Invariants

- HTML is the only view language exposed to application developers.
- Control tags never reach a browser or native renderer as unknown elements.
- Source discovery, route ordering, diagnostics, IR, and artifacts are
  deterministic for identical inputs.
- A route may compose multiple same-level `yon.*` handlers.
- Static fields and method-returned objects contribute to route context.
- Duplicate route-context keys fail compilation. There is no implicit
  last-writer-wins behavior.
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

1. discovers static and dynamic `tac.html` and `yon.html` routes;
2. parses bounded HTML, expressions, control tags, components, slots, events,
   styles, and explicit island hydration;
3. supervises JavaScript, Python, and registered executable Yon handlers and
   collision-checks their static fields and selected method results into route
   context;
4. emits deterministic prerendered web routes, View IR, source maps, island
   modules, shared assets, a service worker, and Route Manifest v1;
5. dispatches exact and dynamic HTTP routes, before/after middleware, workers,
   and durable SSE topic logs from a loopback-safe development server;
6. plans Native UI v1 for macOS, iOS, Android, Linux, and Windows, using
   explicit semantic adapters and the smallest safe local WebSurface fallback;
7. compiles the generated host with the selected platform toolchain and
   publishes capability and artifact manifests atomically; and
8. verifies compatibility, recovery, performance, supply-chain, and release
   lifecycle behavior through executable gates.

The implementation is not yet a supported release. Buildable,
simulator-tested, native-tested, preview, and supported remain distinct
evidence levels in `docs/SUPPORT_TIERS.md`.

## Deliberately Deferred Decisions

These are not hidden gaps in a completed compatibility claim. Each remains a
named product or viability decision:

- OS-level filesystem, network, memory, and CPU isolation for handler children;
- a stable third-party compiler or native-adapter plugin ABI;
- production signing identities and store-specific publication policy;
- whether out-of-scope legacy OpenAPI and telemetry products return in a
  separately owned form.

No deferred decision may be smuggled into production code without an ADR or RFC
when its compatibility or security consequences become concrete.

## Current Non-Goals

- reproducing the legacy client renderer or its in-place component registry;
- implicit imports from application dependency graphs inside handler adapters;
- pooling or reusing handler processes;
- giving remote or local WebSurfaces a native capability bridge;
- treating successful cross-compilation as native runtime evidence;
- publishing a production release before the cutover gate is met.
