# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [26.23.01] — 2026-06-01

### Added

- Yon realtime SSE mailboxes behind `YON_REALTIME_ENABLED=true`.
  `/_yon/realtime/clients` issues durable TTID client IDs,
  `/_yon/realtime/stream` replays queued messages over Server-Sent Events, and
  `/_yon/realtime/messages` stores messages in FYLO's local queue while waking
  any currently connected client stream.
- The example app now includes a Yon realtime messaging slice: MVC routes under
  `/realtime`, FYLO/CHEX schemas for demo clients and messages, and a Tac
  companion script ready for the browser UI markup/styling pass.
- Tac's compiler-injected `fylo` browser client now caches successful
  collection reads in IndexedDB, supports explicit cache policies, falls back
  to cached reads when offline, and invalidates collection cache entries after
  FYLO mutations.
- The example users panel now demonstrates every FYLO browser cache policy and
  includes a temporary create/delete flow that shows mutation invalidation.
- `bun serve` now treats `browser/` + `db/` apps with
  `YON_DATA_BROWSER_ENABLED=true` as a built-in backend, so app authors can use
  the Tac FYLO browser wrapper without adding a `server/` folder.

### Changed

- Upgraded `@d31ma/fylo` to `26.22.7` and migrated Tachyon's FYLO integrations
  to the root-first constructor API.
- Yon compiled route adapters now cache Java, C#, Dart, C++, Swift, Kotlin, and Rust
  build artifacts when `NODE_ENV=production`, keyed by the route file,
  same-language service files, and adapter cache version.
- Yon now supports C++ backend route handlers through `yon.cpp`, generated
  dependency-free `YonJson` helpers, and the compiled route adapter.
- Yon now supports Swift (`yon.swift`) and Kotlin (`yon.kt`) backend route
  handlers through the compiled route adapter. Swift handlers namespace static
  verbs under an `enum`/`struct`/`class Handler` and exchange JSON through
  `Foundation`; Kotlin handlers expose verbs from a `class Handler` companion
  object (or top-level `object Handler`) and receive a generated dependency-free
  `YonJson` helper. Example routes ship under `/languages/swift` and
  `/languages/kotlin`.
- Yon now supports Rust (`yon.rs`) backend route handlers through `rustc`.
  Rust handlers use `struct Handler; impl Handler { pub fn GET(...) { ... } }`,
  receive a generated dependency-free `YonJson` helper, and ship with a
  `/languages/rust` example route/service.
- The example capability atlas now uses a responsive mobile drawer for its
  navigation rail while preserving the persistent desktop rail.

### Fixed

- The example FYLO users panel now redacts API-key-shaped previews before
  rendering them in the browser.

## [26.22.03] — 2026-05-27

### Added

- PostgREST-style query filtering on Fylo browser collection endpoints.
  `GET /_fylo/<collection>/?field=operator.value` supports `eq`, `neq`,
  `gt`, `gte`, `lt`, `lte`, `like`, `ilike`, `in`, `is`, and `not` prefix.
  Reserved params: `select` (vertical filtering), `order` (sorting),
  `limit`, `offset` (pagination).
- The browser example dashboard now includes a Tac input gallery covering
  native input types, `select`, `datalist`, and `textarea`, with live event
  feedback and reactive persisted value bindings.
- The example browser app is now a guided capability atlas linking Tac
  composition, reactive browser state, Yon routes, FYLO storage, telemetry,
  and WebAssembly companions into one navigable application tour.
- A Browser studio panel demonstrates Tac-driven canvas drawing together with
  native semantic `progress`, `meter`, `output`, `time`, and `details`
  surfaces under scoped responsive CSS.
- Tac templates support `:checked="field"` so checkbox and radio state remains
  reactive across component rerenders.
- The checked-in example app now participates in standalone strict type
  checking with its Tac globals and browser dependencies resolved.

### Changed

- **BREAKING**: Yon route handlers use a class-per-route model.
  `export class Handler` with static methods named after HTTP verbs
  (`static async GET(request)`) replaces the prior method-per-directory
  convention (`routes/<path>/GET/yon.<ext>`). All polyglot adapters (JS,
  TS, Python, Ruby, PHP, Java, C#, Dart, C++) support the new pattern.
- Upgraded `@d31ma/fylo` to `^26.22.3`. Removed `getHistory()` calls;
  added `findDeletedDocs()` and `restoreDoc()` to the machine-interface
  demo.
- The Fylo browser query panel now accepts PostgREST-style filter input
  instead of SQL/findDocs modes. The `<fylo-browser>` Tac component and
  the vanilla JS shell both use the new interface.

### Removed

- Removed the `POST /_fylo/api/query` endpoint (SQL and find queries).
  Index-backed filtering now uses PostgREST-style query params on the
  collection URL. The `fylo.sql()` client method is removed.
- Removed Go and Rust language support from example routes and handler
  adapters.
- Removed version history rendering from the Fylo browser (the upstream
  `getHistory()` API was removed in Fylo 26.22.3).

### Fixed

- `HandlerAdapter.hasMethod` regex now matches async generator handlers
  (`static async *GET()`) inside class-per-route definitions.
- `Router.validateSegmentPath` no longer produces false-positive duplicate
  route errors for nested slug routes (e.g. `/items/_id/detail` vs
  `/items/detail`). Slug segments are mapped to a `:` placeholder instead
  of being stripped, preserving path depth in comparisons.
- Static prerendering now preserves literal replacement-token text such as
  Tac's `$` and `$$` persistence sigils instead of expanding it while
  inserting rendered pages into the HTML shell.
- Browser acceptance tests accept `TACHYON_EXAMPLES_E2E_PORT`, avoiding false
  failures when the default example port is already in use.
- The example inventory form now binds CRUD draft state directly to its
  visible native input, so a successful Yon save reliably clears the field.
- Tac client-side navigation preserves query parameters and URL fragments on
  first hydration, intercepted link clicks, and browser history traversal.
- Component stylesheet regression coverage now ensures template-literal
  characters in author CSS comments remain intact in emitted Tac modules.
- Tac `:value` bindings now render loop-local expressions without evaluating
  their identifier outside the loop scope, preventing `ReferenceError` during
  builds such as `<loop :for="option of options"><input :value="option" />`.
- Synthetic value-binding updates preserve `$event.target`,
  `$event.currentTarget`, and `$event.type`, so DOM-style `@input` and
  `@change` handlers continue to work when combined with `:value`.
- `@onMount` registration is deferred until after renderer companion binding,
  preserving mounted callbacks even when an app constructor calls
  `super(props)` without forwarding the injected Tac helper argument.

## [26.21.7] — 2026-05-24

### Added

- New `POST /languages/javascript/fylo` example route that drives the full
  FYLO machine-interface suite (~25 ops). Moved off the lightweight
  `/languages/javascript` dashboard endpoint so diagnostics stays fast.

### Changed

- **BREAKING**: Tac page and component templates now use `tac.html` (with
  optional sibling `tac.js`/`tac.ts`/`tac.css`/`tac.wasm`/`tac.tac.json` and
  source-backed companions). The previous `index.html` convention for Tac
  source is removed; bundled `dist/**/index.html` output is unchanged so
  static hosting and previews behave as before, while dist page/component
  modules emit as `tac.js`.
- **BREAKING**: Yon route handlers move to `server/routes/<path>/<METHOD>/yon.<ext>`.
  The HTTP method is now the parent directory of the handler file, and every
  handler file is named `yon.<ext>` regardless of language. The companion
  `OPTIONS.schema.json` continues to sit as a sibling of the method
  directories. Polyglot class-based handlers (Java, C#, Python, PHP, Ruby)
  derive their class name from the `yon` basename, producing a Pascal-cased
  `Yon` class.
- Repo restructure: `package-contract/` moved to `tests/package-contract/`,
  `stress/` moved to `tests/stress/`, and integration test runners/workers
  moved to `tests/integration/helpers/`. `bun test`, `test:blackbox`, and
  `stress:tac` scripts updated accordingly. Pure layout change with no
  runtime behavior change.
- Upgraded `@d31ma/fylo` to `26.21.6`. The Tachyon-facing surface
  (`new Fylo(fyloOptions(root))`, `Fylo.uniqueTTID`, `executeSQL`,
  `findDocs`, `inspectCollection`) is unchanged; the bump tracks upstream
  bug fixes and the `LocalQueue` / `publish` / `consume` queue exports
  that example apps may consume directly.

### Internal

- Replaced the in-place `tsc --noEmit` typecheck with
  `scripts/typecheck.js`, a wrapper that stages the selected project into
  the OS temp directory on cloud-synced working copies before running the
  same compiler command. `tsconfig.src.json` remains the default release
  gate; pass `tsconfig.tests.json` or `tsconfig.examples.json` to run those
  scoped projects. Controls: `TACHYON_TYPECHECK_TIMEOUT_MS` (default 2
  minutes), `TACHYON_TYPECHECK_STAGE=1` or `0`, and
  `TACHYON_TYPECHECK_KEEP_STAGE=1`.
- Hardened the FYLO browser against traversal-shaped collection names when
  resolving REST-style collection URLs and event-tail files. Invalid
  event-tail collection names return a 200 JSON error in the existing FYLO
  browser API style.
- Removed remaining FYLO browser DOM `innerHTML` rendering for dynamic
  collection, document, history, REST result, encrypted-field, and event
  values; the UI now writes dynamic values with text nodes.
- Restored `tsconfig.json` to inherit from `tsconfig.base.json` so Bun's
  runtime module resolver picks up the `@/*` → `./examples/server/*`
  path alias used by the example polyglot route handlers.
- `Pool.prewarmAllHandlers` fallback handler path updated to the new
  `<METHOD>/<routeFileName>` layout. The previous `<METHOD>` fallback
  silently produced non-existent paths after the Yon rename and dropped
  most polyglot handlers from the warmed-process pool.
- Restored `static chexSchemas = new Map()` on `Validate`; the schema
  validator's response-matching path requires it, and the working tree
  had it removed mid-refactor.
- `FyloMachineRepository.ensureDemoSchema()` now writes to a
  `fylo-demo-items` collection instead of `items`, so the demo run no
  longer overwrites the real items schema that other example routes
  validate against.

## [26.21.05] — 2026-05-22

### Added

- Stage 3 decorator forms of the `Tac` runtime helpers, available as bare
  identifiers in companion scripts: `@inject(key, fallback?)`,
  `@provide(key)`, `@env(key, fallback?)`, `@onMount`, and `@emit(name)`. The
  Tachyon compiler auto-imports these into companion scripts when it sees the
  `@<name>` syntax, so user code references them without an `import` line.
  Outside of companion scripts (tests, library code), import them from the
  new `@d31ma/tachyon/decorators` package export. The existing instance-method
  API (`this.inject`, `this.provide`, `this.onMount`, …) is unchanged.
- Automatic prop-to-field binding: `Tac` copies values from `this.props` onto
  any same-named instance field the subclass declared, removing the need for
  boilerplate `this.foo = this.props.foo ?? default` assignments in the
  constructor. A leading `$` on the field name is stripped when matching, so
  `$`-prefixed persistent fields auto-bind to their unprefixed prop
  counterparts (`$clicks` ↔ `props.clicks`); a direct match against the
  prefixed key still takes precedence. Only fields the subclass explicitly
  declared participate, and `props` and `tac` are skipped so a malicious
  prop object cannot overwrite framework state.
- New `./decorators` package export resolving to `src/runtime/decorators.js`.

### Changed

- The build-time compiler class previously exported as `Tac` from the
  `./compiler` entry is now exported as `Compiler`, and its source moved from
  `src/compiler/template-compiler.js` to `src/compiler/index.js`. The
  `./compiler` package export still resolves; consumers who imported the
  default export under the name `Tac` should rename the binding to `Compiler`
  to avoid colliding with the runtime `Tac` class.
- Internal reorganization of `src/server/`: flat files moved into
  responsibility-scoped subdirectories. `route-handler.js`, `browser-env.js`,
  and `schema-validator.js` are now under `src/server/http/`; `openapi.js`
  under `src/server/openapi/`; `logger.js`, `console-logger.js`, and
  `telemetry.js` under `src/server/observability/`; `process-pool.js` under
  `src/server/process/`. `yon.js` stays at the `src/server/` root.
  `package.json#main`, `#types`, and `exports."."` were updated to the new
  `src/server/http/route-handler.js` path; `exports."./server"` is unchanged.
- Build-time manifests (`route-manifest.json`, `shell-manifest.json`) moved
  out of `src/runtime/` into `src/shared/manifests/` to separate generated
  artifacts from runtime source. Compiler path constants and `.gitignore`
  updated.

### Removed

- Unused `Compiler` import from `src/cli/serve.js`. The compiler module has
  no top-level side effects, so the import was strictly dead.
- Orphan `layout-manifest.json`. The compiler writes `Compiler.layoutMapping`
  to `shell-manifest.json`; nothing in the codebase ever read or wrote
  `layout-manifest.json`. Removed from `src/shared/manifests/` and
  `.gitignore`.

### Fixed

- Upgraded `@d31ma/fylo` to `26.21.2` (bringing `@d31ma/chex` `26.21.02`),
  which introduced a `SchemaValidator` API requiring `.schema.json` extension
  in schema references and `path:`-prefixed cache keys. Aligned
  `validateWithChex` to seed the CHEX cache with the expected key format.
- Updated `Router.optionsFileName` from `OPTIONS.json` to `OPTIONS.schema.json`
  so the route handler discovers the renamed schema files.
- Renamed route `OPTIONS.json` files to `OPTIONS.schema.json` and schema
  history files (`v1.json`) to `v1.schema.json` for consistency with the
  framework's naming convention. Test env file moved from root to
  `tests/.env.test`.

## [2.0.0]

Tachyon 2.0 is a full rewrite. The framework splits into `Tac` (frontend) and
`Yon` (backend/runtime), the source moves from TypeScript to JavaScript with
strict JSDoc, and the CLI surface is renamed. Consumers upgrading from 1.x
should expect to touch imports, bin names, and any code that assumed the old
`dist/layouts/` output.

### Breaking Changes

- Source rewritten from TypeScript to JavaScript + JSDoc. Package `main`,
  `types`, and the `.`, `./server`, `./compiler` exports now resolve to `.js`
  files.
- `./server` export now points at `src/server/yon.js` (previously
  `process-executor.ts`).
- CLI binaries renamed:
  - `tach.init` → `yon.init`
  - `tach.serve` → `yon.serve`
  - `tach.bundle` → `tac.bundle`
  - `tach.preview` → `tac.preview`
- Removed `dist/layouts/` output. Page shells are now represented through
  `shells.json`, and the runtime uses a single app shell template with the HMR
  client injected only in development.

### Added

- Polyglot backend handlers: any executable file with a shebang can serve a
  route, receiving JSON on stdin.
- OOP-style companion classes with `export default class extends Tac`, plus
  support for anonymous subclasses.
- Automatic `sessionStorage` persistence for `$`-prefixed instance fields,
  keyed by module path + page/component identity + field name.
- Local-first browser `fetch()` inside Tac page/component scripts, backed by
  IndexedDB with read caching for `GET`/`HEAD` and mutation-aware invalidation.
- Explicit browser env allowlisting through `TAC_PUBLIC_ENV` and
  `this.env(key, fallback)`.
- Generated OpenAPI 3.1 document at `/openapi.json` and a self-hosted Tachyon
  docs UI at `/api-docs` with request auth, filtering, deep links, cURL
  generation, and live "try it out" execution.
- FYLO-backed OpenTelemetry storage with request and handler span correlation,
  `traceparent` continuation, and response `Traceparent` / `X-Trace-Id` headers.
  Configured through `OTEL_ENABLED`, `OTEL_FYLO_ROOT`, `OTEL_SERVICE_NAME`, and
  `OTEL_CAPTURE_IP`.
- Scoped component CSS via `@scope ([data-tac-scope="..."])` wrappers.
- Shared frontend surface: `browser/shared/assets/*`, `browser/shared/data/*`,
  `browser/shared/scripts/main.js` entry, and `browser/shared/styles/*`.
- Built-in health/readiness endpoints: `/health`, `/healthz`, `/ready`,
  `/readyz`.
- Shape-aware `yon.serve`: `browser/`-only bundles and serves the frontend,
  `server/`-only serves backend routes, and apps with both run full-stack on
  one port.
- Tac template runtime with `{expr}`, `{!expr}`, `@event`, `:prop`,
  `:value`, `<loop :for>`, `<logic :if>`, and `<my-component lazy />`.
- `Tac` helpers: `env`, `fetch`, `emit`, `inject`, `provide`, `onMount`,
  `rerender`, `isBrowser`, `isServer`, `props`.
- Distributed rate limiting via an exported `rateLimiter` from `middleware.js`
  (Upstash Redis supported out of the box); in-memory limiter retained for
  single-instance deployments.
- Production hardening: Bun password verification for hashed Basic Auth,
  request body and parameter limits, handler timeout enforcement, JWT expiry
  rejection when decodable, route request/response validation through
  `OPTIONS`, configurable CSP, optional HSTS.
- Proxy-aware request context, origin-aware CORS rejection before handler
  execution, and document-request detection using `Sec-Fetch-*` headers with
  `Accept: text/html` as fallback.
- Cache headers for runtime assets, chunks, shared assets, and shared data.
- Inferred content type on handler responses (`application/json` for
  JSON-shaped output, `text/plain` otherwise).
- `tac.bundle --watch` and `tac.preview --watch` for iterative static builds.

### Changed

- Publishing moved to GitHub Packages under `@d31ma/tachyon`.
- Scaffolds are now JavaScript-first with strict JSDoc; TypeScript companion
  scripts remain supported at runtime.

### Fixed

- `findDocs('otel-spans')` call in the telemetry integration test now passes
  the required `StoreQuery` argument so `tsc --noEmit` stays clean against
  `@d31ma/fylo` 2.3.

## [1.11.1] and earlier

See the Git history and GitHub release notes for pre-2.0 changes.

[Unreleased]: https://github.com/d31ma/Tachyon/compare/v26.22.03...HEAD
[26.22.03]: https://github.com/d31ma/Tachyon/compare/v26.21.7...v26.22.03
[26.21.7]: https://github.com/d31ma/Tachyon/compare/v26.21.05...v26.21.7
[26.21.05]: https://github.com/d31ma/Tachyon/compare/v2.0.0...v26.21.05
[2.0.0]: https://github.com/d31ma/Tachyon/compare/v1.11.1...v2.0.0
[1.11.1]: https://github.com/d31ma/Tachyon/releases/tag/v1.11.1
