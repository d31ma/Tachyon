# Tachyon

Tachyon is a polyglot, file-system-routed full-stack framework for [Bun](https://bun.sh).

- `Tac` is the frontend layer.
- `Yon` is the backend/runtime layer.

## Features

- Polyglot backend handlers with executable files and shebangs
- Tac pages and components with `index.html` / `component.html` templates
- Companion `*.js`, `*.ts`, and `*.css` files beside templates
- OOP-style companion classes with `export default class extends Tac`
- Automatic session persistence for `$`-prefixed instance fields
- Local-first browser `fetch()` for Tac page/component scripts with IndexedDB-backed read caching and mutation-aware invalidation
- Explicit browser env allowlisting through `TAC_PUBLIC_ENV` and `this.env(...)`
- Static export with prerendered `dist/**/index.html`
- Shared frontend assets under `/shared/assets/*`
- Shared frontend data under `/shared/data/*`
- Generated OpenAPI 3.1 docs at `/openapi.json` with a self-hosted Tachyon docs UI at `/api-docs`
- FYLO-backed OpenTelemetry storage with request and handler span correlation
- Built-in health/readiness endpoints
- Proxy-aware request context, CORS enforcement, and optional rate limiting

## Install

```bash
bun add @d31ma/tachyon
```

If you install from GitHub Packages, configure `.npmrc` first:

```text
@d31ma:registry=https://npm.pkg.github.com
//npm.pkg.github.com/:_authToken=${GITHUB_TOKEN}
```

## Quick Start

```bash
yon.init my-app
cd my-app
bun install
bun run serve
```

Useful commands:

```bash
bun run serve
bun run bundle
bun run preview
```

`bun run serve` is shape-aware: `browser/` only bundles and serves the frontend, `server/` only serves backend routes, and apps with both folders run as a full-stack app on one port.

## Scaffold Layout

```text
browser/
  pages/
    index.html
    index.js
    index.css
  components/
    hero.html
    hero.css
  shared/
    scripts/
    styles/
    assets/
    data/

server/
  routes/
  data/
  deps/
```

Scaffolds are JavaScript-first and use strict JSDoc rather than TypeScript source files.
The runtime still supports TypeScript companion scripts when you want them.

The example app in [examples/](examples/) demonstrates Tac and Yon working together:

- reactive page state
- persisted `$` fields
- local-first fetches
- backend handlers in multiple languages
- shared data, shared assets, and a browser entry
- middleware, OpenAPI docs, route manifests, and component companions

## Configuration

Create a `.env` file in your app root. All variables are optional.

```env
PORT=8000
HOST=127.0.0.1
HOSTNAME=127.0.0.1
DEV=true
LOG_LEVEL=info
LOG_FORMAT=pretty
TRUST_PROXY=
TAC_FORMAT=esm

ALLOW_HEADERS=Content-Type,Authorization
ALLOW_ORIGINS=
ALLOW_CREDENTIALS=false
ALLOW_EXPOSE_HEADERS=
ALLOW_MAX_AGE=3600
ALLOW_METHODS=GET,POST,PUT,DELETE,PATCH,OPTIONS

BASIC_AUTH=
BASIC_AUTH_HASH=
VALIDATE=true
CONTENT_SECURITY_POLICY=default-src 'self'
ENABLE_HSTS=false

HANDLER_TIMEOUT_MS=30000
MAX_BODY_BYTES=1048576
MAX_PARAM_LENGTH=1000
RATE_LIMIT_MAX=
RATE_LIMIT_WINDOW_MS=

ROUTES_PATH=server/routes
PAGES_PATH=browser/pages
COMPONENTS_PATH=browser/components
ASSETS_PATH=browser/shared/assets
SHARED_SCRIPTS_PATH=browser/shared/scripts
SHARED_STYLES_PATH=browser/shared/styles
SHARED_DATA_PATH=browser/shared/data
MIDDLEWARE_PATH=./middleware
```

Notes:

- `TRUST_PROXY=loopback` is a good default when running behind a local reverse proxy.
- Set both `RATE_LIMIT_MAX` and `RATE_LIMIT_WINDOW_MS` to enable the built-in in-memory limiter.
- For distributed deployments, export a custom `rateLimiter` from `middleware.js`.
- Prefer `BASIC_AUTH_HASH` over plaintext `BASIC_AUTH` in production.

Generate a Bun password hash with:

```bash
bun -e "console.log(await Bun.password.hash('user:pass'))"
```

## Backend Routing

Yon backend routes live in `server/routes`.

```text
server/routes/
  GET                  -> GET /
  POST                 -> POST /
  api/
    GET                -> GET /api
    POST               -> POST /api
    _version/
      GET              -> GET /api/:version
  items/
    GET                -> GET /items
    POST               -> POST /items
    DELETE             -> DELETE /items
  OPTIONS              -> route schema file
```

Rules:

- handler files must be executable
- the last segment must be an uppercase HTTP method file such as `GET` or `POST`
- dynamic route segments use `_slug` on disk and become `:slug` at runtime
- the first segment cannot be dynamic
- adjacent dynamic segments are not allowed

Each handler receives JSON on `stdin`:

```json
{
  "headers": {},
  "body": {},
  "query": {},
  "paths": {},
  "context": {
    "requestId": "req-id",
    "ipAddress": "127.0.0.1",
    "protocol": "http",
    "host": "127.0.0.1:8000"
  }
}
```

## Frontend Routing

Tac page routes live in `browser/pages`.

```text
browser/pages/
  index.html           -> /
  docs/
    index.html         -> /docs
  blog/
    _slug/
      index.html       -> /blog/:slug
```

If an ancestor page contains `<slot />`, it acts as a reusable shell for descendant pages.

Tac components live in `browser/components`.

```text
browser/components/
  clicker.html
  clicker.js
  clicker.css
```

Companion scripts can be JavaScript or TypeScript:

- `clicker.js`
- `clicker.ts`

## Tac Templates

Templates support:

- `{expr}` for escaped interpolation
- `{!expr}` for trusted raw HTML
- `@event="handler()"` for event binding
- `:prop="expr"` for dynamic attributes
- `:value="field"` for two-way input binding
- `<loop :for="...">`
- `<logic :if="...">`
- `<my-component />`
- `<my-component lazy />`

Example page:

```html
<!-- browser/pages/index.html -->
<section class="hero">
  <h1>{headline}</h1>
  <p>{subtitle}</p>
  <button @click="refresh()">Refresh</button>
</section>

<clicker label="Visits" />
```

```js
// browser/pages/index.js
export default class extends Tac {
  /** @type {number} */
  $visits = 0
  /** @type {string} */
  headline = 'Tac + Yon'
  /** @type {string} */
  subtitle = 'Reactive frontend, polyglot backend.'

  constructor(props = {}, tac = undefined) {
    super(props, tac)
    this.$visits += 1
    if (this.isBrowser) document.title = 'Home'
  }

  async refresh() {
    const response = await this.fetch('/api')
    const payload = await response.json()
    this.subtitle = String(payload.message ?? this.subtitle)
  }
}
```

Anonymous companion classes are fully supported:

```js
export default class extends Tac {}
```

## Tac Companion Scripts

Companion scripts are instantiated automatically during render. Their fields and methods are visible in the matching HTML template without the developer manually referencing the class instance.

Companion authors only need to think about `Tac` itself. Internal runtime helper plumbing is attached by the framework and does not need to be imported, typed, or threaded through user code.

Available helpers through `Tac`:

- `this.env(key, fallback?)`
- `this.fetch(input, init)`
- `this.emit(name, detail)`
- `this.inject(key, fallback?)`
- `this.provide(key, value)`
- `this.onMount(fn)`
- `this.rerender()`
- `this.isBrowser`
- `this.isServer`
- `this.props`

### Browser Environment Variables

Tac can expose explicitly public browser config through `this.env(key, fallback)`.

```js
export default class extends Tac {
  apiBase = this.env('PUBLIC_API_BASE_URL', '/api')
}
```

Set the allowlist with `TAC_PUBLIC_ENV`:

```bash
TAC_PUBLIC_ENV=PUBLIC_API_BASE_URL,PUBLIC_SENTRY_DSN
PUBLIC_API_BASE_URL=https://api.example.com
```

Important boundary:

- anything sent to browser JavaScript can be seen by the browser user
- Tachyon therefore only exposes vars you explicitly allowlist
- private secrets must stay in Yon and be used through server routes, middleware, or upstream API calls made on the server

There is no secure way to give a browser script a secret and also keep that secret hidden from the browser.

## API Docs

Yon exposes an OpenAPI 3.1 document at `/openapi.json` and a self-hosted Tachyon docs UI at `/api-docs`.

- route response schemas are derived from each route's `OPTIONS` file
- request schemas can also flow into the OpenAPI document when you define `req` or `request`
- the docs page is rendered by Tachyon-owned HTML, CSS, and JavaScript instead of a third-party docs bundle
- the docs UI supports request authorization, operation filtering, deep links, cURL generation, and live "try it out" execution with response inspection

## OpenTelemetry Storage

Yon can persist OpenTelemetry trace data into FYLO without adding an SDK dependency stack.

```bash
OTEL_ENABLED=true
OTEL_FYLO_ROOT=.tachyon-otel
OTEL_SERVICE_NAME=@d31ma/tachyon
```

When enabled, Yon writes request spans and nested handler spans into the FYLO collection `otel-spans`.

- incoming `traceparent` headers are continued when present
- responses emit `Traceparent` and `X-Trace-Id` for correlation
- spans fail open: telemetry write failures are logged but do not fail the request
- `OTEL_CAPTURE_IP=true` opt-in is required before client IPs are stored
- each FYLO record stores the exact OTLP JSON `TracesData` payload in `otlpJson`, plus scalar index fields such as `traceId`, `spanId`, and `requestId`
- custom Tachyon-specific correlation stays namespaced in span attributes such as `tachyon.request.id`

### Testing Telemetry

Integration coverage already exists in [tests/integration/api-routes.test.js](tests/integration/api-routes.test.js).

Run the focused check with:

```bash
bun test tests/integration/api-routes.test.js
```

For a manual smoke test:

```bash
cd examples
OTEL_ENABLED=true \
OTEL_FYLO_ROOT=.tachyon-otel \
OTEL_SERVICE_NAME=tachyon-dev \
BASIC_AUTH_HASH="$(bun -e "console.log(await Bun.password.hash('admin:pass'))")" \
bun ../src/cli/serve.js
```

Then send a traced request:

```bash
curl -i \
  -H 'Authorization: Basic YWRtaW46cGFzcw==' \
  -H 'X-Request-Id: manual-otel-test' \
  -H 'traceparent: 00-0123456789abcdef0123456789abcdef-1111111111111111-01' \
  http://127.0.0.1:8000/api
```

You should see:

- `Traceparent` and `X-Trace-Id` in the response headers
- persisted FYLO documents under `.tachyon-otel/otel-spans/.fylo/`
- one server span and one nested handler span for the request

### Consuming Telemetry From FYLO

The example app includes a Yon telemetry consumer at `/telemetry`.

- it reads `otel-spans` from FYLO
- parses the stored `otlpJson` payload back into OTLP JSON `TracesData`
- returns a monitoring-friendly summary plus recent spans

That route is implemented in [examples/server/routes/telemetry/GET](examples/server/routes/telemetry/GET), and the example dashboard uses it to render a live telemetry panel.

The examples also include a tiny alerting worker at [examples/server/workers/telemetry-alert-worker.js](examples/server/workers/telemetry-alert-worker.js).

Run it against the example app with:

```bash
cd examples
TELEMETRY_URL=http://127.0.0.1:8000/telemetry?limit=25 \
BASIC_AUTH_HEADER='Basic YWRtaW46cGFzcw==' \
ALERT_SLOW_MS=500 \
ALERT_STATUS_CODE=500 \
bun run telemetry:alerts
```

It polls the telemetry endpoint, flags slow routes and server errors, and prints structured JSON that can be shipped to another service or cron job.

### `$` Field Persistence

`$`-prefixed instance fields are automatically persisted to `sessionStorage`.

```js
export default class extends Tac {
  /** @type {number} */
  $count = 0

  increment() {
    this.$count += 1
  }
}
```

The persistence key is generated from:

- the Tac module path
- the current page path or generated component instance identity
- the field name

That makes the key stable across reloads and unique per persisted field instance.

### Local-First `fetch()`

Inside Tac page/component scripts only, `fetch()` is wrapped with a local-first strategy:

- all request methods are supported and forwarded through the Tac wrapper
- successful `GET` and `HEAD` responses are written to IndexedDB
- later `GET` and `HEAD` calls read from the cache first
- `cache: 'reload'` bypasses the cached read
- successful non-`GET` requests invalidate cached `GET` and `HEAD` entries for the same URL so later reads do not serve stale data

This does not override the global browser `fetch` outside Tac page/component execution.

On the Yon side, handler responses are also given an inferred content type now:

- JSON-looking output is served as `application/json`
- other string output falls back to `text/plain`

### Scoped Component CSS

`component.css` is automatically wrapped with a component scope:

```css
@scope ([data-tac-scope="clicker"]) { ... }
```

That scope is applied to the generated wrapper around each component instance.

## Shared Frontend Files

- `browser/shared/assets/*` is served at `/shared/assets/*`
- `browser/shared/data/*` is served at `/shared/data/*`
- `browser/shared/scripts/main.js` is the optional browser entry
- `browser/shared/styles/*` is available for imports from `main.js`

If `main.js` imports CSS, Tachyon emits `/main.css` and links it from generated HTML shells.

The example app uses a local `main.js` plus shared assets/data, and loads Tailwind's browser build from `https://cdn.jsdelivr.net/npm/@tailwindcss/browser@4` for demo convenience. Tailwind's own docs note that browser CDN is for development, not production.

## Build Output

`tac.bundle` writes a static-ready `dist/` directory.

Typical output:

```text
dist/
  index.html
  docs/index.html
  pages/index.js
  pages/docs/index.js
  components/clicker.js
  modules/*.js
  shared/assets/*
  shared/data/*
  shells.json
  routes.json
  spa-renderer.js
  main.js
  main.css
```

Notes:

- there is no `dist/layouts/` output in v2
- page shells are represented through `shells.json`
- static assets are emitted under `dist/shared/assets/`
- the runtime now uses one app shell template and injects the HMR client only in development

## Commands

- `yon.serve` detects `browser/` and `server/` contents and serves the frontend, backend, or full-stack app
- `tac.bundle` builds `dist/`
- `tac.bundle --watch` keeps `dist/` fresh
- `tac.preview` serves `dist/`
- `tac.preview --watch` rebuilds and previews frontend output together

## Operations

Built-in endpoints:

- `/health`
- `/healthz`
- `/ready`
- `/readyz`

Tachyon also supports:

- origin-aware CORS rejection before handler execution
- proxy-aware request context
- in-memory rate limiting
- middleware-provided distributed rate limiting
- cache headers for runtime assets, chunks, shared assets, and shared data
- document-request detection using browser navigation headers such as `Sec-Fetch-Dest` / `Sec-Fetch-Mode`, with `Accept: text/html` kept as a fallback

## Distributed Rate Limiting

Export a `rateLimiter` from `middleware.js` to use a shared backend.

Required env vars:

```env
UPSTASH_REDIS_REST_URL=
UPSTASH_REDIS_REST_TOKEN=
RATE_LIMIT_MAX=100
RATE_LIMIT_WINDOW_MS=60000
UPSTASH_RATE_LIMIT_PREFIX=tachyon:rate-limit
```

## Security

- security headers on all responses
- Bun password verification for hashed Basic Auth
- request body and parameter limits
- handler timeout enforcement
- JWT expiry rejection when decodable
- route request/response validation through `OPTIONS`

## Production Notes

- prefer `BASIC_AUTH_HASH`
- set explicit `ALLOW_ORIGINS`
- configure `TRUST_PROXY` when behind nginx, Caddy, or Cloudflare
- use a shared rate limiter for multi-instance deployments
- validate the built frontend with `tac.preview` before deploy

## License

MIT
