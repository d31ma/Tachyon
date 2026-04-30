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

That installs the public stable release from npm by default.

If you are a `d31ma` member and want the private beta channel from GitHub Packages instead, configure a user-level `.npmrc`:

```ini
# ~/.npmrc
@d31ma:registry=https://npm.pkg.github.com
//npm.pkg.github.com/:_authToken=${GITHUB_PACKAGES_TOKEN}
always-auth=true
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
YON_PORT=8000
YON_HOST=127.0.0.1
YON_HOSTNAME=127.0.0.1
YON_DEV=true
YON_LOG_LEVEL=info
YON_LOG_FORMAT=pretty
YON_TRUST_PROXY=
TAC_FORMAT=esm

YON_ALLOW_HEADERS=Content-Type,Authorization
YON_ALLOW_ORIGINS=
YON_ALLOW_CREDENTIALS=false
YON_ALLOW_EXPOSE_HEADERS=
YON_ALLOW_MAX_AGE=3600
YON_ALLOW_METHODS=GET,POST,PUT,DELETE,PATCH,OPTIONS

YON_BASIC_AUTH=
YON_BASIC_AUTH_HASH=
YON_VALIDATE=true
YON_CONTENT_SECURITY_POLICY=default-src 'self'
YON_ENABLE_HSTS=false

YON_HANDLER_TIMEOUT_MS=30000
YON_MAX_BODY_BYTES=1048576
YON_MAX_PARAM_LENGTH=1000
YON_RATE_LIMIT_MAX=
YON_RATE_LIMIT_WINDOW_MS=

YON_ROUTES_PATH=server/routes
YON_PAGES_PATH=browser/pages
YON_COMPONENTS_PATH=browser/components
YON_ASSETS_PATH=browser/shared/assets
YON_SHARED_SCRIPTS_PATH=browser/shared/scripts
YON_SHARED_STYLES_PATH=browser/shared/styles
YON_SHARED_DATA_PATH=browser/shared/data
YON_MIDDLEWARE_PATH=./middleware

FYLO_ROOT=db/collections
FYLO_SCHEMA_DIR=db/schemas
FYLO_INDEX_BACKEND=s3-prefix
FYLO_S3_ACCESS_KEY_ID=test
FYLO_S3_SECRET_ACCESS_KEY=test
FYLO_S3_REGION=us-east-1
FYLO_S3_ENDPOINT=http://localhost:4566

YON_DATA_BROWSER_ENABLED=false
YON_DATA_BROWSER_READONLY=true
YON_DATA_BROWSER_REVEAL=false
```

Notes:

- `YON_TRUST_PROXY=loopback` is a good default when running behind a local reverse proxy.
- Set both `YON_RATE_LIMIT_MAX` and `YON_RATE_LIMIT_WINDOW_MS` to enable the built-in in-memory limiter.
- For distributed deployments, export a custom `rateLimiter` from `middleware.js`.
- Prefer `YON_BASIC_AUTH_HASH` over plaintext `YON_BASIC_AUTH` in production.
- FYLO-owned storage settings use the `FYLO_*` prefix because they are consumed by `@d31ma/fylo`.
- Tachyon-owned runtime settings use `YON_*` or `TAC_*`; avoid mixed prefixes such as `YON_FYLO_*`.

Generate a Bun password hash with:

```bash
bun -e "console.log(await Bun.password.hash('user:pass'))"
```

## LocalStack for FYLO S3 Indexes

Tachyon's checked-in `.env.test` uses LocalStack for FYLO `s3-prefix` indexes:

```env
FYLO_INDEX_BACKEND=s3-prefix
FYLO_S3_ACCESS_KEY_ID=test
FYLO_S3_SECRET_ACCESS_KEY=test
FYLO_S3_REGION=us-east-1
FYLO_S3_ENDPOINT=http://localhost:4566
```

Start LocalStack and create one local S3 bucket per FYLO collection before running tests or the LocalStack-backed example seed:

```bash
bun run localstack:up
bun run localstack:buckets
bun --env-file=.env.test test
```

For the example app, use:

```bash
cd examples
bun --env-file=.env.localstack run seed
bun --env-file=.env.localstack run serve
```

Useful maintenance commands:

```bash
bun run localstack:down
docker compose -f docker-compose.localstack.yml down -v
```

`@d31ma/fylo@26.18.29` uses each collection name directly as its S3 bucket name for `s3-prefix` indexes. LocalStack keeps those names local, so generic collection buckets such as `items`, `users`, and `otel-spans` do not collide with AWS's global bucket namespace.

## Backend Routing

Yon backend routes live in `server/routes`.

```text
server/
  routes/                         -> thin request/response controllers
    languages/
      javascript/GET              -> GET /languages/javascript
      javascript/POST             -> POST /languages/javascript
      javascript/PUT              -> PUT /languages/javascript
      typescript/GET              -> GET /languages/typescript
      python/GET                  -> GET /languages/python
      ruby/GET                    -> GET /languages/ruby
      php/GET                     -> GET /languages/php
      go/GET                      -> GET /languages/go
      csharp/GET                  -> GET /languages/csharp
      java/POST                   -> POST /languages/java
      dart/DELETE                 -> DELETE /languages/dart
      rust/PATCH                  -> PATCH /languages/rust
      python/versions/_version/GET -> GET /languages/python/versions/:version
      typescript/items/GET        -> GET /languages/typescript/items
      typescript/items/POST       -> POST /languages/typescript/items
      typescript/items/DELETE     -> DELETE /languages/typescript/items
      javascript/telemetry/GET    -> GET /languages/javascript/telemetry
      OPTIONS.json                -> route schema files
  services/                       -> application/business logic
  repositories/                   -> database and persistence access
  data/                           -> local example data
```

The examples intentionally use an MVC-style backend dependency direction:
`routes -> services -> repositories`. Route files should stay small and call a
service. Services coordinate validation, business rules, and multiple
dependencies. Repositories are the only layer that talks directly to persistence
or runtime data sources. The `/languages/*` example is now the single backend
showcase: it includes polyglot handlers, CRUD item routes, dynamic routes, and a
telemetry consumer.

Rules:

- handler files should use their language extension, such as `GET.js`, `POST.ts`, or `PATCH.rs`
- the filename stem must be an uppercase HTTP method such as `GET` or `POST`
- dynamic route segments use `_slug` on disk and become `:slug` at runtime
- the first segment cannot be dynamic
- adjacent dynamic segments are not allowed

Yon route files expose a `handler(request)` function. The HTTP method comes from
the filename, so `server/routes/languages/typescript/items/GET.ts` handles
`GET /languages/typescript/items` and the function name stays consistent across
every method and language.

```ts
import ItemService from '../../../../services/item-service.ts'

const service = new ItemService()

export async function handler() {
  return service.listItems()
}
```

Class-based handlers are also supported for languages that prefer that shape:

```js
export default class GET {
  async handler(request) {
    return { message: 'Hello from Yon' }
  }
}
```

Handlers return plain data, FastAPI-style. `OPTIONS.json` decides which response
status the returned body matches. Yon serializes the returned value, validates it
when `YON_VALIDATE` is enabled, applies CORS/security headers, and writes the
process response internally.

Every handler receives this request object:

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

If an inbound `X-Request-Id` header is present, Yon preserves it for upstream
correlation. Otherwise Yon generates a TTID request ID using `@d31ma/ttid`.

Yon invokes pure function/class handlers directly for dynamic runtimes and
generates tiny build wrappers for compiled/static runtimes. No third-party
adapter dependency is added; compiled handlers use the language toolchain already
on the developer or deployment machine.

```text
Language      Supported handler shape
JavaScript    export function handler(request) / default class GET.handler()
TypeScript    export function handler(request) / default class GET.handler()
Python        def handler(request) / class GET.handler()
Ruby          def handler(request) / class GET#handler
PHP           function handler($request) / class GET::handler
Dart          handler(Map<String, dynamic> request)
Go            func Handler(request map[string]any) any
Java          POST.handler(Map<String, Object> request)
C#            GET.Handler(JsonElement request)
Rust          pub fn handler(request: &JsonValue) -> impl Display
```

Java and Rust intentionally stay dependency-free. Yon generates a tiny JSON
adapter beside the compiled wrapper:

- Java receives a `java.util.Map<String, Object>` or `Object`.
- Rust receives `&crate::yon_json::JsonValue` for ergonomic object access.

Rust handlers can import the generated type from the wrapper crate:

```rust
use crate::yon_json::JsonValue;

pub fn handler(request: &JsonValue) -> JsonValue {
    let request_id = request
        .get("context")
        .and_then(|context| context.get("requestId"))
        .and_then(JsonValue::as_str)
        .unwrap_or("unknown");

    JsonValue::String(format!("request: {}", request_id))
}
```

`OPTIONS.json` files validate both incoming requests and outgoing responses with
CHEX regex schemas. Yon does not add Tachyon-specific type shorthands:
every string leaf is passed to CHEX as the regex pattern to validate.

```json
{
  "POST": {
    "request": {
      "body": {
        "name": "^[a-z0-9-]+$",
        "quantity": "^[0-9]+$"
      }
    },
    "response": {
      "201": {
        "id": "^[0-9A-Za-z_-]+$",
        "name": "^.{1,120}$",
        "quantity": "^[0-9]+$"
      },
      "400": {
        "detail": "^.+$"
      }
    }
  }
}
```

Numeric status codes may live directly under the method object, which is the
style used by the example app.

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
    const response = await this.fetch('/languages/javascript')
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

### Decorator Form

The same context, lifecycle, and event helpers are also exposed as Stage 3 decorators. They move the wiring out of the constructor and onto the field or method that owns the value. Companion scripts can use them as bare identifiers — the Tachyon compiler auto-imports them when it sees the `@<name>` syntax, so no `import` line is needed in user code.

```js
export default class extends Tac {
  /** @type {string} */
  label = 'Interactions'

  /** @type {string | undefined} */
  @inject('demo-release', 'Tac')
  release

  /** @type {string} */
  @provide('demo-release')
  appVersion = 'TACHYON'

  /** @type {number | undefined} */
  @env('PUBLIC_PORT', 3000)
  port

  @onMount
  refresh() { /* runs once after the component is attached */ }

  @emit('saved')
  async save(payload) { return await this.fetch('/languages/typescript/items', { method: 'POST', body: JSON.stringify(payload) }) }
}
```

Decorator semantics:

- `@inject(key, fallback?)` — field decorator; field is initialized from `tac.inject(key, fallback)`.
- `@provide(key)` — field decorator; the field's initial value is registered with `tac.provide(key, value)` after construction.
- `@env(key, fallback?)` — field decorator; field is initialized from `tac.env(key, fallback)`.
- `@onMount` — method decorator; the method is registered as an `onMount` handler bound to the instance.
- `@emit(name)` — method decorator; the method's return value (or its resolved value, for async methods) is emitted as `name`. Rejections propagate without emitting.

`@inject` and `@env` mirror the underlying `tac.inject` / `tac.env` types and may return `undefined` when no fallback is supplied; declare the field's JSDoc type accordingly.

Outside of companion scripts (tests, library code), import the decorators explicitly:

```js
import { inject, provide, env, onMount, emit } from '@d31ma/tachyon/decorators'
```

### Reactive Fields

Tac companion fields are reactive in the browser. Assigning to a declared instance field schedules one batched rerender automatically, so app code does not need to call `this.rerender()` after normal state changes:

```js
export default class extends Tac {
  count = 0

  increment() {
    this.count += 1
  }
}
```

`$`-prefixed persistent fields are reactive too, and still write through to `sessionStorage`. `this.rerender()` remains available for rare cases where code mutates nested object/array contents in place instead of assigning a new field value.

### Prop Auto-Binding

`Tac` automatically copies values from `this.props` onto any same-named instance field declared on the subclass. A leading `$` on the field name is stripped when matching, so a `$`-prefixed persistent field automatically pairs with the unprefixed prop key:

```js
export default class extends Tac {
  /** @type {string} */
  label = 'Default'           // populated from props.label, falls back to 'Default'

  /** @type {number} */
  count = 0                   // populated from props.count, falls back to 0

  /** @type {number} */
  $clicks = 0                 // populated from props.clicks (leading $ stripped on match)
}
```

The binding runs after child class fields initialize, so the prop value wins over the field's declared default. Only fields the subclass explicitly declared participate — extra prop keys are ignored, and `props` and `tac` are skipped so a malicious-shaped props object cannot overwrite framework state. Direct matches take precedence over the stripped form, so a `$clicks` field paired with a `props.$clicks` prop uses the prefixed value rather than the unprefixed one.

### Browser Environment Variables

Tac can expose explicitly public browser config through `this.env(key, fallback)`.

```js
export default class extends Tac {
  apiBase = this.env('PUBLIC_API_BASE_URL', '/languages/javascript')
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

- route response schemas are derived from each route's `OPTIONS.json` file
- request schemas can also flow into the OpenAPI document when you define `request`
- the docs page is rendered by Tachyon-owned HTML, CSS, and JavaScript instead of a third-party docs bundle
- the docs UI supports request authorization, operation filtering, deep links, cURL generation, and live "try it out" execution with response inspection

## OpenTelemetry Storage

Yon can persist OpenTelemetry trace data into FYLO without adding an SDK dependency stack.

```bash
YON_OTEL_ENABLED=true
YON_OTEL_ROOT=.tachyon-otel
YON_OTEL_SERVICE_NAME=@d31ma/tachyon
```

When enabled, Yon writes request spans and nested handler spans into the FYLO collection `otel-spans`.

- incoming `traceparent` headers are continued when present
- responses emit `Traceparent` and `X-Trace-Id` for correlation
- spans fail open: telemetry write failures are logged but do not fail the request
- `YON_OTEL_CAPTURE_IP=true` opt-in is required before client IPs are stored
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
YON_OTEL_ENABLED=true \
YON_OTEL_ROOT=.tachyon-otel \
YON_OTEL_SERVICE_NAME=tachyon-dev \
YON_BASIC_AUTH_HASH="$(bun -e "console.log(await Bun.password.hash('admin:pass'))")" \
bun ../src/cli/serve.js
```

Then send a traced request:

```bash
curl -i \
  -H 'Authorization: Basic YWRtaW46cGFzcw==' \
  -H 'X-Request-Id: manual-otel-test' \
  -H 'traceparent: 00-0123456789abcdef0123456789abcdef-1111111111111111-01' \
  http://127.0.0.1:8000/languages/javascript
```

You should see:

- `Traceparent` and `X-Trace-Id` in the response headers
- persisted FYLO documents under `.tachyon-otel/otel-spans/.fylo/`
- one server span and one nested handler span for the request

### Consuming Telemetry From FYLO

The example app includes a Yon telemetry consumer at `/languages/javascript/telemetry`.

- it reads `otel-spans` from FYLO
- parses the stored `otlpJson` payload back into OTLP JSON `TracesData`
- returns a monitoring-friendly summary plus recent spans

That route is implemented in [examples/server/routes/languages/javascript/telemetry/GET.js](examples/server/routes/languages/javascript/telemetry/GET.js), and the example dashboard uses it to render a live telemetry panel.

The examples also include a tiny alerting worker at [examples/server/workers/telemetry-alert-worker.js](examples/server/workers/telemetry-alert-worker.js).

Run it against the example app with:

```bash
cd examples
YON_TELEMETRY_URL=http://127.0.0.1:8000/languages/javascript/telemetry?limit=25 \
YON_BASIC_AUTH_HEADER='Basic YWRtaW46cGFzcw==' \
YON_ALERT_SLOW_MS=500 \
YON_ALERT_STATUS_CODE=500 \
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
- `browser/shared/scripts/imports.js` is the optional browser entry
- `browser/shared/styles/*` is available for imports from `imports.js`

If `imports.js` imports CSS, Tachyon emits `/imports.css` and links it from generated HTML shells.

The example app uses a local `imports.js` plus shared assets/data. Keep demo-only browser helpers out of published runtime code; shared production styles should live under `browser/shared/assets` or `browser/shared/styles`.

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
  imports.js
  imports.css
```

Notes:

- there is no `dist/layouts/` output
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
YON_RATE_LIMIT_MAX=100
YON_RATE_LIMIT_WINDOW_MS=60000
UPSTASH_RATE_LIMIT_PREFIX=tachyon:rate-limit
```

## Security

- security headers on all responses
- Bun password verification for hashed Basic Auth
- request body and parameter limits
- handler timeout enforcement
- JWT expiry rejection when decodable
- route request/response validation through `OPTIONS.json`

## Production Notes

- prefer `YON_BASIC_AUTH_HASH`
- set explicit `YON_ALLOW_ORIGINS`
- configure `YON_TRUST_PROXY` when behind nginx, Caddy, or Cloudflare
- use a shared rate limiter for multi-instance deployments
- validate the built frontend with `tac.preview` before deploy

## License

MIT
