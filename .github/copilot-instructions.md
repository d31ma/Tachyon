# TACHYON — Project Guidelines

## Overview

TACHYON (`@vyckr/tachyon`) is a polyglot, file-system-routed full-stack framework for Bun. It lets you define API routes as plain executable files written in any language, and build reactive front-end pages with a lightweight HTML template syntax (Yon) — all without configuration.

**Assume a long-running server model** (Bun.serve). This means:
- Static state (route tables, pre-warmed processes) is initialised once at startup and reused across requests
- HMR watches the filesystem and hot-reloads routes and components without restarting the process
- Handler processes are pre-spawned (`prewarmHandler`) and recycled — avoid creating new subprocesses per request when possible
- Keep the startup path fast — defer non-critical work with `setImmediate`

## Architecture

### File-System Routing

- Routes are directories under `routes/`; the last path segment is an **uppercase HTTP method** (e.g. `GET`, `POST`, `DELETE`) or `HTML` for a front-end page
- Dynamic segments start with `:` (e.g. `:version`, `:id`) — first segment must not be dynamic, and adjacent dynamic segments are not allowed
- An `OPTIONS` file in a route directory defines request/response validation schemas
- Route handlers are **executable files** — any language with a shebang line is supported

### Core Modules

| Module | Responsibility |
|--------|---------------|
| `src/server/route-handler.ts` | Route discovery, validation, slug parsing, request processing |
| `src/cli/serve.ts` | Server entry point — `Bun.serve`, HMR watcher |
| `src/cli/bundle.ts` | Static build — renders GET routes into `dist/` |
| `src/server/process-executor.ts` | Handler execution — subprocess spawning, stream/response draining, schema validation |
| `src/server/process-pool.ts` | Process pre-warming and lifecycle management |
| `src/server/schema-validator.ts` | Request/response schema validation against OPTIONS definitions |
| `src/server/console-logger.ts` | Leveled console logger with timestamps |
| `src/compiler/template-compiler.ts` | Build-time HTML→JS compiler — components, pages, assets, npm dependency bundling |
| `src/compiler/render-template.js` | Component render template — ID generation, event invocation, value binding |
| `src/runtime/spa-renderer.ts` | Client-side DOM morphing, event delegation, SPA navigation |
| `src/runtime/hot-reload-client.ts` | HMR client — SSE listener for live reload |

### Folder Structure

```
src/
  cli/
    serve.ts              # Server entry point (Bun.serve + HMR)
    bundle.ts             # Static site builder
  compiler/
    template-compiler.ts  # Build-time HTML→JS template compiler
    render-template.js    # Component render function skeleton
  runtime/
    spa-renderer.ts       # Client-side DOM morphing and SPA renderer
    hot-reload-client.ts  # HMR SSE client
    shells/
      development.html    # Dev-mode HTML shell
      production.html     # Production HTML shell
      not-found.html      # Default 404 page
  server/
    route-handler.ts      # Route discovery and request processing
    process-executor.ts   # Polyglot handler execution and process management
    process-pool.ts       # Process pre-warming and recycling
    schema-validator.ts   # Request/response validation
    console-logger.ts     # Console logger override
tests/
  integration/
    api-routes.test.ts    # Route integration tests
    browser-interactions.test.ts  # Playwright browser E2E tests
    server-worker.ts      # Test server worker (spawns serve.ts in a subprocess)
    stream-helper.ts      # SSE streaming test helper
```

### Dependencies

- **`Bun.Glob`** — Route file discovery via glob scanning (`**/{METHOD}` patterns)
- **`HTMLRewriter`** — Server-side HTML component parsing and transformation
- **`Bun.spawn`** — Polyglot handler execution via subprocesses with piped stdio

## Engineering Standards

- **SOLID principles**: Single responsibility per class/method, depend on abstractions, open for extension without modifying core logic
- **Clean code**: Descriptive naming, small focused functions, no dead code or commented-out blocks, DRY without premature abstraction
- **Test discipline**: When changing `src/` code, update or add corresponding tests in `tests/` — never leave tests stale after a behaviour change
- **Error handling**: Fail fast with meaningful errors at system boundaries; validate route structures at startup
- **No magic values**: Use constants or environment variables; avoid hardcoded strings/numbers in logic
- **Type safety**: Leverage TypeScript's type system fully — avoid `any` in implementation code, prefer narrow types, and validate at I/O boundaries

## Code Style

- **Runtime**: Bun (ESNext target, ES modules)
- **Strict TypeScript**: `strict: true`, `noImplicitReturns`, `isolatedModules`
- Prefer `class` with `static` methods for modules (no standalone functions)
- Use `default export` for primary classes (`Router`, `Tach`, `Yon`)
- Interfaces (`RequestContext`, `RouteResponse`, `RequestPayload`, `RouteOptions`) are defined in `src/server/router.ts`
- Constants are `UPPER_SNAKE_CASE` (e.g. `HMR_DEBOUNCE_MS`, `STREAM_MIME_TYPE`)

## Build & Test

```bash
bun test           # Run all tests
bun run start      # Start the dev server
bun run bundle     # Build static site into dist/
```

- Tests use `bun:test` — `describe`, `test`, `expect`, `beforeAll`
- The test server is launched via a `Worker` that spawns `src/serve.ts` with the `examples/` directory as cwd
- Tests exercise routes defined in `examples/routes/` against `http://localhost:8080`
- Tests validate HTTP status codes across multiple methods per route

## Conventions

- Route handlers receive request context on **stdin as JSON** and write response body to **stdout**
- Errors are written to **stderr** — the server returns them as 500 responses
- SSE streaming is triggered by `Accept: text/event-stream` header
- The `HTML` method designates a front-end page (rendered by Yon), not an HTTP method
- Components use a trailing underscore in tag names (e.g. `<myComp_ />`) and live in `components/` as `.html` files
- Template syntax: `{expr}` for interpolation, `@event` for event binding, `:value` for two-way binding, `<loop>` / `<logic>` for control flow
- Node modules in front-end code must be imported dynamically with the `/modules/` prefix

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `PORT` | Server port (default: `8080`) |
| `HOSTNAME` | Server hostname (default: `0.0.0.0`) |
| `TIMEOUT` | Idle timeout in seconds |
| `DEV` | Enable development mode |
| `ROUTES_PATH` | Custom routes directory (default: `<cwd>/routes`) |
| `COMPONENTS_PATH` | Custom components directory (default: `<cwd>/components`) |
| `ASSETS_PATH` | Custom assets directory (default: `<cwd>/assets`) |
| `ALLOW_HEADERS` | CORS `Access-Control-Allow-Headers` |
| `ALLOW_ORIGINS` | CORS `Access-Control-Allow-Origin` |
| `ALLOW_CREDENTIALS` | CORS `Access-Control-Allow-Credentials` |
| `ALLOW_EXPOSE_HEADERS` | CORS `Access-Control-Expose-Headers` |
| `ALLOW_MAX_AGE` | CORS `Access-Control-Max-Age` |
| `ALLOW_METHODS` | Allowed HTTP methods (default: `GET,POST,PUT,DELETE,PATCH,HEAD`) |
| `BASIC_AUTH` | Basic auth credentials (`username:password`) |
| `VALIDATE` | Enable request/response schema validation |
