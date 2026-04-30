# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/d31ma/Tachyon/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/d31ma/Tachyon/compare/v1.11.1...v2.0.0
[1.11.1]: https://github.com/d31ma/Tachyon/releases/tag/v1.11.1
