<p align="center">
  <a href="https://www.npmjs.com/package/@d31ma/tachyon"><img src="https://img.shields.io/npm/v/@d31ma/tachyon?style=flat&label=npm" alt="npm version"></a>
  <a href="https://bun.sh"><img src="https://img.shields.io/badge/runtime-bun-f9f1e0?style=flat&logo=bun" alt="bun"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat" alt="license"></a>
</p>

<h1 align="center">Tachyon</h1>

<p align="center">
  A polyglot, file-system-routed full-stack framework for <a href="https://bun.sh">Bun</a>.<br/>
  <code>Tac</code> is the frontend layer. <code>Yon</code> is the backend/runtime layer.
</p>

---

## Features

- Polyglot backend handlers with executable files and shebangs
- Tac pages and components with `tac.html` templates
- Companion `*.js`, `*.ts`, and `*.css` files beside templates
- OOP-style companion classes with `export default class extends Tac`
- Browser-local Tac Workers compiled in-house to `tac.wasm` (no external toolchain), invoked with `fetch("tac://...")`
- Automatic persistence for `$`-prefixed (sessionStorage) and `$$`-prefixed (localStorage) instance fields
- Local-first browser `fetch()` plus worker-owned OPFS-backed FYLO document mirrors for Tac page/component scripts
- Explicit browser env allowlisting through `TAC_PUBLIC_ENV` and `this.env(...)`
- Static export with prerendered `dist/**/index.html`
- Shared frontend assets under `/shared/assets/*`
- Shared frontend data under `/shared/data/*`
- Generated OpenAPI 3.1 docs at `/openapi.json` with a self-hosted Tachyon docs UI at `/api-docs`
- FYLO-backed OpenTelemetry storage with request and handler span correlation
- Built-in health/readiness endpoints
- Proxy-aware request context, CORS enforcement, and optional rate limiting

---

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

---

## Quick Start

```bash
yon.init my-app
cd my-app
bun install
bun run serve
```

Useful commands:

<table>
<tr><th align="left">Command</th><th align="left">Description</th></tr>
<tr><td><code>bun run serve</code></td><td>Shape-aware dev server — frontend, backend, or full-stack on one port</td></tr>
<tr><td><code>bun run bundle</code></td><td>Build static <code>dist/</code> output</td></tr>
<tr><td><code>bun run preview</code></td><td>Serve the built <code>dist/</code> directory</td></tr>
</table>

`bun run serve` is shape-aware: `browser/` only bundles and serves the frontend,
`server/` only serves backend routes, apps with both folders run as a full-stack
app on one port, and `browser/` + `db/` apps mount Tachyon's built-in FYLO
browser routes when `YON_DATA_BROWSER_ENABLED=true`.

---

## Scaffold Layout

```text
browser/
  pages/
    tac.html
    tac.js
    tac.css
  components/
    hero/
      tac.html
      tac.css
  shared/
    scripts/
    styles/
    assets/
    data/
  workers/
    language/
      rust/
        tac.rs

server/
  routes/
    GET/
      yon.js
  data/
  deps/
```

Scaffolds are JavaScript-first and use strict JSDoc rather than TypeScript source files.
The runtime still supports TypeScript companion scripts when you want them.

The example app in [examples/](examples/) demonstrates Tac and Yon working together:

- a guided capability atlas joining native HTML/CSS/JavaScript surfaces with
  working Tac, Yon, and FYLO flows rather than isolated code snippets
- reactive page state
- accessible native controls and a reactive canvas studio with semantic
  `progress`, `meter`, `output`, `time`, and `details` elements
- persisted `$` (sessionStorage) and `$$` (localStorage) fields
- local-first fetches and OPFS-backed FYLO browser reads
- browser-local Tac Workers for heavier frontend work
- frontend-only external SSE streaming with reactive Tac updates
- backend handlers in multiple languages
- shared data, shared assets, and a browser entry
- middleware, OpenAPI docs, embedded route manifests, and component companions

---

<details>
<summary><h2 style="display:inline">Configuration</h2></summary>

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
YON_CORS_ORIGIN=
YON_ALLOW_CREDENTIALS=false
YON_ALLOW_EXPOSE_HEADERS=
YON_ALLOW_MAX_AGE=3600
YON_ALLOW_METHODS=GET,POST,PUT,DELETE,PATCH,OPTIONS

YON_BASIC_AUTH=
YON_BASIC_AUTH_HASH=
YON_VALIDATE=true
YON_CONTENT_SECURITY_POLICY=default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'
YON_ENABLE_HSTS=false
YON_SKIP_BUNDLE=false

YON_HANDLER_TIMEOUT_MS=30000
YON_MAX_BODY_BYTES=1048576
YON_MAX_PARAM_LENGTH=1000
YON_RATE_LIMIT_MAX=
YON_RATE_LIMIT_WINDOW_MS=

YON_ROUTES_PATH=server/routes
YON_PAGES_PATH=browser/pages
YON_COMPONENTS_PATH=browser/components
YON_WORKERS_PATH=browser/workers
YON_ASSETS_PATH=browser/shared/assets
YON_SHARED_SCRIPTS_PATH=browser/shared/scripts
YON_SHARED_STYLES_PATH=browser/shared/styles
YON_SHARED_DATA_PATH=browser/shared/data
YON_MIDDLEWARE_PATH=./middleware

FYLO_ROOT=db
FYLO_SCHEMA_DIR=db/schemas
FYLO_INDEX_BACKEND=local-fs

YON_DATA_BROWSER_ENABLED=false
YON_DATA_BROWSER_READONLY=true
YON_DATA_BROWSER_REVEAL=false

YON_REALTIME_ENABLED=false
YON_REALTIME_PATH=/_yon/realtime
YON_REALTIME_POLL_MS=1000
YON_REALTIME_HEARTBEAT_MS=15000
YON_REALTIME_MAX_EVENT_BYTES=65536
```

> **Notes:**
>
> - `YON_TRUST_PROXY=loopback` is a good default when running behind a local reverse proxy.
> - Set both `YON_RATE_LIMIT_MAX` and `YON_RATE_LIMIT_WINDOW_MS` to enable the built-in in-memory limiter.
> - For distributed deployments, export a custom `rateLimiter` from `middleware.js`.
> - Prefer `YON_BASIC_AUTH_HASH` over plaintext `YON_BASIC_AUTH` in production.
> - FYLO-owned storage settings use the `FYLO_*` prefix because they are consumed by `@d31ma/fylo`.
> - Tachyon-owned runtime settings use `YON_*` or `TAC_*`; avoid mixed prefixes such as `YON_FYLO_*`.

Generate a Bun password hash with:

```bash
bun -e "console.log(await Bun.password.hash('user:pass'))"
```

</details>

---

<details>
<summary><h2 style="display:inline">FYLO Storage</h2></summary>

Tachyon uses `@d31ma/fylo@26.22.7`, which is filesystem-first and uses the
FYLO `local-fs` index backend by default. Set `FYLO_ROOT` to the directory that
should contain FYLO-managed collections:

```env
FYLO_ROOT=db
FYLO_SCHEMA_DIR=db/schemas
FYLO_INDEX_BACKEND=local-fs
```

Run the example seed and server with the normal example environment:

```bash
cd examples
bun --env-file=.env run serve
```

FYLO owns everything inside `FYLO_ROOT`, including document shards, local prefix
indexes, event journals, locks, and WORM history. With `local-fs`, each
collection stores compact index files under
`<FYLO_ROOT>/.collections/<collection>/index/`, so no external indexing service is
required.

When `YON_DATA_BROWSER_ENABLED=true`, the FYLO browser mounts at `/_fylo` and
also exposes Django-style collection URLs with `_fylo` replacing `api`:

```bash
curl -X POST http://localhost:8000/_fylo/books/ \
  -H 'Content-Type: application/json' \
  -d '{"title":"Tachyon Patterns"}'
curl http://localhost:8000/_fylo/books/
curl http://localhost:8000/_fylo/books/<id>/
curl -X PUT http://localhost:8000/_fylo/books/<id>/ \
  -H 'Content-Type: application/json' \
  -d '{"title":"Tachyon Patterns","status":"published"}'
curl -X PATCH http://localhost:8000/_fylo/books/<id>/ \
  -H 'Content-Type: application/json' \
  -d '{"status":"published"}'
curl -X DELETE http://localhost:8000/_fylo/books/<id>/
```

Mutating methods require `YON_DATA_BROWSER_READONLY=false`. FYLO is immutable
under the hood, so successful `PUT` and `PATCH` responses return the new
document id in `{ "id": "..." }`.

Keep `YON_DATA_BROWSER_ENABLED=false` in production unless the route is also
protected by `YON_BASIC_AUTH_HASH`, explicit origins (`YON_ALLOW_ORIGINS` or
`YON_CORS_ORIGIN`), and a shared rate limiter. Event-tail collection names are
constrained before FYLO event files are read; invalid names return a 200 JSON
error, matching the existing FYLO browser API style.

`FYLO_INDEX_BACKEND=s3-client` is also passed through to FYLO when you want FYLO
to store index keys through Bun's S3 client. The old `s3-prefix`/LocalStack
configuration is intentionally rejected so stale deployment env cannot silently
fall back to a different backend.

</details>

---

<details>
<summary><h2 style="display:inline">Yon Realtime</h2></summary>

Yon can expose WebSocket-like realtime delivery over Server-Sent Events (SSE)
with FYLO-backed durable mailboxes. Enable it explicitly:

```env
YON_REALTIME_ENABLED=true
YON_REALTIME_PATH=/_yon/realtime
FYLO_ROOT=db
```

The built-in endpoints are:

```bash
# Register a durable browser/client identifier.
curl -X POST http://localhost:8000/_yon/realtime/clients

# Listen for messages. Browsers can also use EventSource with the same URL.
curl -N \
  -H 'Accept: text/event-stream' \
  'http://localhost:8000/_yon/realtime/stream?clientId=<TTID>'

# Send a message to a client mailbox.
curl -X POST http://localhost:8000/_yon/realtime/messages \
  -H 'Content-Type: application/json' \
  -d '{"to":"<TTID>","event":"chat.message","data":{"text":"hello"}}'
```

This is intentionally not a true WebSocket. Yon cannot open a connection back to
an offline browser. Instead, clients keep or reopen an SSE connection. Messages
are persisted into a FYLO local queue under the runtime root; if the target
client is connected, Yon drains the queue immediately into that stream. If the
server restarts or the browser reconnects later, the client sends its
`Last-Event-ID` or `cursor` and Yon replays stored messages after that offset.

Realtime uses `YON_REALTIME_ROOT` when it is set; otherwise it shares
`FYLO_ROOT`, then falls back to Tachyon's default `.fylo-data` store. Leave
`YON_REALTIME_ROOT` unset unless realtime mailboxes need their own storage root.

Keep this route protected in production with `YON_BASIC_AUTH_HASH`, explicit
CORS origins, and rate limiting. Client identifiers must be FYLO TTIDs, event
names are allowlisted, and payload size is bounded by
`YON_REALTIME_MAX_EVENT_BYTES`.

</details>

---

## Backend Routing

Yon backend routes live in `server/routes`.

```text
server/
  routes/                         -> thin request/response controllers
    languages/
      javascript/yon.js           -> GET/POST/PUT /languages/javascript
      typescript/yon.ts           -> GET /languages/typescript
      python/yon.py               -> GET /languages/python
      ruby/yon.rb                 -> GET /languages/ruby
      php/yon.php                 -> GET /languages/php
      csharp/yon.cs               -> GET /languages/csharp
      cpp/yon.cpp                 -> GET /languages/cpp
      swift/yon.swift             -> GET /languages/swift
      kotlin/yon.kt               -> GET /languages/kotlin
      rust/yon.rs                 -> GET /languages/rust
      java/yon.java               -> POST /languages/java
      dart/yon.dart               -> DELETE /languages/dart
      python/versions/_version/yon.py -> GET /languages/python/versions/:version
      typescript/items/yon.ts     -> GET/POST /languages/typescript/items
      typescript/items/_id/yon.ts -> GET/PUT/PATCH/DELETE /languages/typescript/items/:id
      javascript/telemetry/yon.js -> GET /languages/javascript/telemetry
      OPTIONS.schema.json         -> route schema files
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

Every non-JavaScript/TypeScript language route demonstrates FYLO access through the `fylo.exec`
machine interface bundled by `@d31ma/fylo`. These examples extend existing
routes instead of adding separate FYLO endpoints: the route calls a service, the
service calls a repository or language-native process helper, and that layer
sends JSON operations to the FYLO binary. During development they invoke `bunx
--bun fylo.exec`; in production, set `FYLO_EXEC_PATH=/path/to/fylo` to point
those helpers at a compiled FYLO executable instead.

The language examples collectively exercise the FYLO machine interface:

| Language route | Friendly collection | FYLO binary operations shown |
| -------------- | ------------------- | ---------------------------- |
| `GET /languages/javascript` | `fylo-operation-runs`, `fylo-related-records`, `fylo-disposable-runs` | full operation suite: `executeSQL`, `createCollection`, `dropCollection`, `inspectCollection`, `rebuildCollection`, `getDoc`, `getLatest`, `getHistory`, `findDocs`, `joinDocs`, `putData`, `batchPutData`, `patchDoc`, `patchDocs`, `delDoc`, `delDocs`, `importBulkData`, `schemaInspect`, `schemaCurrent`, `schemaHistory`, `schemaDoctor`, `schemaValidate`, `schemaMaterialize` |
| `GET /languages/typescript` | `language-route-events` | `createCollection`, `putData`, `findDocs` |
| `GET /languages/python` | `language-route-events` | `createCollection`, `putData`, `findDocs` |
| `GET /languages/ruby` | `language-route-events` | `createCollection`, `putData`, `executeSQL` |
| `GET /languages/php` | `language-route-events` | `createCollection`, `batchPutData`, `findDocs` |
| `GET /languages/csharp` | `fylo-demo-items` | `schemaCurrent`, `schemaHistory` |
| `GET /languages/cpp` | `language-route-events` | `createCollection`, `inspectCollection`, `rebuildCollection` |
| `GET /languages/swift` | `language-route-events`, `language-route-relations` | `createCollection`, `putData`, `joinDocs` |
| `GET /languages/kotlin` | `language-route-events` | `createCollection`, `batchPutData`, `patchDocs` |
| `GET /languages/rust` | `fylo-rust-disposable` | `createCollection`, `inspectCollection`, `dropCollection` |
| `POST /languages/java` | `language-route-events` | `createCollection`, `putData`, `getLatest` |
| `DELETE /languages/dart` | `language-route-events` | `createCollection`, `importBulkData` |

Rules:

- handler files live at `<route>/yon.<ext>` and export or define `class Handler`
- HTTP verbs are public static methods such as `GET`, `POST`, `PUT`, and `DELETE`
- the `OPTIONS.schema.json` schema file sits beside the route handler file
- dynamic route segments use `_slug` on disk and become `:slug` at runtime
- the first segment cannot be dynamic
- adjacent dynamic segments are not allowed

Yon route files expose a `handler(request)` function. The HTTP method comes from
the parent directory, so `server/routes/languages/typescript/items/GET/yon.ts`
handles `GET /languages/typescript/items` and the function name stays consistent
across every method and language.

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

Handlers return plain data, FastAPI-style. `OPTIONS.schema.json` decides which response
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
correlation. Otherwise Yon generates a TTID request ID through `@d31ma/fylo`.

Yon invokes `Handler` classes through small runtime adapters. Dynamic runtimes
load the route module directly; compiled/static runtimes generate tiny build
wrappers that call the public static HTTP method on the class. No third-party
adapter dependency is added; compiled handlers use the language toolchain already
on the developer or deployment machine.

When `NODE_ENV=production`, Yon caches compiled Java, C#, Dart, C++, Swift, Kotlin, and Rust
artifacts per handler fingerprint. The fingerprint includes the route file, copied same-language
service files, and the adapter cache version, so a later `POST` to the same
compiled route can reuse the artifact prepared by an earlier `GET` while source
changes still invalidate the cache.

<table>
<tr><th align="left">Language</th><th align="left">Supported handler shape</th></tr>
<tr><td>JavaScript</td><td><code>export class Handler { static GET(request) {} }</code></td></tr>
<tr><td>TypeScript</td><td><code>export class Handler { static GET(request) {} }</code></td></tr>
<tr><td>Python</td><td><code>class Handler: @staticmethod def GET(request)</code></td></tr>
<tr><td>Ruby</td><td><code>class Handler; def self.GET(request); end; end</code></td></tr>
<tr><td>PHP</td><td><code>class Handler { public static function GET($request) {} }</code></td></tr>
<tr><td>Dart</td><td><code>class Handler { static GET(Map&lt;String, dynamic&gt; request) {} }</code></td></tr>
<tr><td>Java</td><td><code>public class Handler { public static Object GET(Map&lt;String, Object&gt; request) {} }</code></td></tr>
<tr><td>C#</td><td><code>public class Handler { public static object GET(JsonElement request) {} }</code></td></tr>
<tr><td>C++</td><td><code>class Handler { public: static YonJson GET(const YonJson&amp; request) {} };</code></td></tr>
<tr><td>Swift</td><td><code>enum Handler { static func GET(_ request: [String: Any]) -> Any? {} }</code></td></tr>
<tr><td>Kotlin</td><td><code>class Handler { companion object { fun GET(request: Map&lt;String, Any?&gt;): Any? {} } }</code></td></tr>
<tr><td>Rust</td><td><code>struct Handler; impl Handler { pub fn GET(request: &amp;YonJson) -&gt; YonJson {} }</code></td></tr>
</table>

Java, C++, Kotlin, and Rust intentionally stay dependency-free. Yon generates a tiny JSON
adapter beside the compiled wrapper; Swift uses the JSON support already in
`Foundation`:

- Java receives a `java.util.Map<String, Object>` or `Object`.
- C++ receives a generated `YonJson` helper with object lookup, scalar coercion,
  and JSON serialization.
- Swift receives a `[String: Any]` decoded with `Foundation`'s `JSONSerialization`
  and may return a `[String: Any]`, array, scalar, or `String`.
- Kotlin receives a `Map<String, Any?>` from a generated `YonJson` helper and may
  return a `Map`, `List`, scalar, or `String`. Handlers expose verbs through a
  `companion object` (or a top-level `object Handler`).
- Rust receives a generated `YonJson` helper and exposes verbs as associated
  functions on `impl Handler`, mirroring static methods without requiring a
  third-party crate.

`OPTIONS.schema.json` files validate both incoming requests and outgoing responses with
CHEX regex schemas. Yon does not add Tachyon-specific type shorthands:
every string leaf is passed to CHEX as the regex pattern to validate.
Because this is strict CHEX validation, nested objects and arrays of objects are
valid schema shapes for route request/response validation.

```json
{
  "POST": {
    "request": {
      "body": {
        "orderId": "^ORD-[0-9]+$",
        "items": [
          {
            "sku": "^[A-Z0-9-]+$",
            "quantity": "^[1-9][0-9]*$"
          }
        ]
      }
    },
    "response": {
      "201": {
        "id": "^[0-9A-Za-z_-]+$",
        "orderId": "^ORD-[0-9]+$",
        "items": [
          {
            "sku": "^[A-Z0-9-]+$",
            "quantity": "^[1-9][0-9]*$"
          }
        ]
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

---

## Frontend Routing

Tac page routes live in `browser/pages`.

```text
browser/pages/
  tac.html             -> /
  docs/
    tac.html           -> /docs
  blog/
    _slug/
      tac.html         -> /blog/:slug
```

If an ancestor page contains `<slot />`, it acts as a reusable shell for descendant pages.

Tac components live in `browser/components`.

```text
browser/components/
  clicker/
    tac.html
    tac.js
    tac.css
```

Companion scripts can be JavaScript or TypeScript:

- `tac.js`
- `tac.ts`

Tac uses one component naming convention: each component folder segment is
lowercase alphanumeric and has a `tac.html` template. The component tag is
the folder path joined with hyphens:

```text
browser/components/clicker/tac.html       -> <clicker />
browser/components/panel/users/tac.html   -> <panel-users />
```

Flat templates and hyphenated folder names such as
`browser/components/clicker.html` and `browser/components/panel-users/tac.html`
are rejected so app structure, generated module paths, CSS scopes, and template
tags all use the same naming rule.

---

## Tac Templates

Templates support:

- `{expr}` for escaped interpolation
- `{!expr}` for trusted raw HTML
- `@event="handler()"` for event binding
- `:prop="expr"` for dynamic attributes
- `:value="field"` for two-way input binding
- `:checked="field"` for reactive checkbox and radio state
- `<loop :for="...">`
- `<logic :if="...">`
- `<switch :value="...">` with `<case :when="...">` and `<case default>`
- `<my-component />`
- `<my-component lazy />`

Template expressions run inside Tac's async render function, so `await` is
available in interpolation, dynamic attributes, and control expressions. Prefer
companion-script fields for uncached network data so rerenders do not repeatedly
fetch the same resource.

Example page:

```html
<!-- browser/pages/tac.html -->
<section class="hero">
  <h1>{headline}</h1>
  <p>{subtitle}</p>
  <button @click="refresh()">Refresh</button>
</section>

<clicker label="Visits" />

<switch :value="status">
  <case :when="['loading', 'pending']">
    <p>Working...</p>
  </case>
  <case :when="Status.Ready">
    <p>Ready.</p>
  </case>
  <case default>
    <p>Unknown state.</p>
  </case>
</switch>
```

```js
// browser/pages/tac.js
const Status = {
  Loading: 'loading',
  Pending: 'pending',
  Ready: 'ready'
}

export default class extends Tac {
  /** @type {number} */
  $visits = 0
  Status = Status
  status = Status.Loading
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

---

## Tac Companion Scripts

Companion scripts are instantiated automatically during render. Their fields and methods are visible in the matching HTML template without the developer manually referencing the class instance.

Companion authors only need to think about `Tac` itself. Internal runtime helper plumbing is attached by the framework and does not need to be imported, typed, or threaded through user code.

Available helpers through `Tac`:

- `this.env(key, fallback?)`
- `this.fetch(input, init)`
- `this.onMount(fn)`
- `this.publish(name, value, options?)`
- `this.rerender()`
- `this.subscribe(name, callbackOrFallback?, options?)`
- `this.isBrowser`
- `this.isServer`
- `this.props`

### Decorator Form

The same context, lifecycle, and event helpers are also exposed as Stage 3 decorators. They move the wiring out of the constructor and onto the field or method that owns the value. Companion scripts can use them as bare identifiers — the Tachyon compiler auto-imports them when it sees the `@<name>` syntax, so no `import` line is needed in user code.

For editor and `checkJs` support in consuming apps, include Tachyon's ambient
globals once in the app:

```ts
/// <reference types="@d31ma/tachyon/globals" />
```

The `yon.init` scaffold writes this to `tachyon-env.d.ts` automatically. It
lets app-authored page and component scripts use bare `Tac`, `publish`,
`subscribe`, `env`, `onMount`, `fylo`, and `Worker` without local imports
or `Cannot find name` diagnostics from TypeScript-aware tooling.

If the app also uses plain ESLint `no-undef`, import Tachyon's globals map in
the app's flat config:

```js
import tachyonGlobals from '@d31ma/tachyon/eslint-globals'

export default [{
  files: ['browser/**/*.{js,ts}'],
  languageOptions: { globals: tachyonGlobals }
}]
```

```js
export default class extends Tac {
  /** @type {string} */
  label = 'Interactions'

  /** @type {string | undefined} */
  @subscribe
  release

  /** @type {string} */
  @publish('release')
  appVersion = 'TACHYON'

  /** @type {number | undefined} */
  @env('PUBLIC_PORT', 3000)
  port

  @subscribe('demo-refresh', { onMount: true })
  refresh() { /* runs once after mount and again when demo-refresh publishes */ }

  @publish('saved')
  async save(payload) { return await this.fetch('/languages/typescript/items', { method: 'POST', body: JSON.stringify(payload) }) }
}
```

Decorator semantics:

<table>
<tr><th align="left">Decorator</th><th align="left">Kind</th><th align="left">Behavior</th></tr>
<tr><td><code>@subscribe</code> or <code>@subscribe(name, fallback?)</code></td><td>field</td><td>Field initialized from the retained signal value, falls back when the signal has not been published, and updates when the signal changes. Bare <code>@subscribe</code> uses the field name as the signal name.</td></tr>
<tr><td><code>@subscribe</code>, <code>@subscribe(name, options?)</code>, or <code>@subscribe(options)</code></td><td>method</td><td>Method receives every future publication for the signal. Bare <code>@subscribe</code> uses the method name as the signal name. Pass <code>{ onMount: true }</code> to also run once after the component/page mounts.</td></tr>
<tr><td><code>@publish</code> or <code>@publish(name)</code></td><td>field</td><td>Initial field value is retained as the signal value and future assignments publish retained updates. Bare <code>@publish</code> uses the field name as the signal name.</td></tr>
<tr><td><code>@publish</code>, <code>@publish(name, options?)</code>, or <code>@publish(options)</code></td><td>method</td><td>Return value (or resolved value for async) is published as <code>name</code>. Bare <code>@publish</code> uses the method name as the signal name. Rejections propagate without publishing.</td></tr>
<tr><td><code>@env(key, fallback?)</code></td><td>field</td><td>Field initialized from <code>tac.env(key, fallback)</code></td></tr>
<tr><td><code>@onMount</code></td><td>method</td><td>Method registered as an <code>onMount</code> handler bound to the instance</td></tr>
</table>

`@subscribe` and `@env` mirror the underlying `tac.subscribe` / `tac.env` types and may return `undefined` when no fallback is supplied; declare the field's JSDoc type accordingly.

Outside of companion scripts (tests, library code), import the decorators explicitly:

```js
import { subscribe, publish, env, onMount } from '@d31ma/tachyon/decorators'
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

`$`-prefixed and `$$`-prefixed persistent fields are reactive too, and still write through to `sessionStorage` / `localStorage`. Strict v2 reactivity is assignment-based: mutate nested object/array contents by assigning a new field value rather than relying on a manual render escape hatch.

### Prop Auto-Binding

`Tac` automatically copies values from `this.props` onto any same-named instance field declared on the subclass. A leading `$` or `$$` on the field name is stripped when matching, so a `$`-prefixed or `$$`-prefixed persistent field automatically pairs with the unprefixed prop key:

```js
export default class extends Tac {
  /** @type {string} */
  label = 'Default'           // populated from props.label, falls back to 'Default'

  /** @type {number} */
  count = 0                   // populated from props.count, falls back to 0

  /** @type {number} */
  $clicks = 0                 // populated from props.clicks (leading $ stripped on match)
  /** @type {string} */
  $$theme = 'light'           // populated from props.theme (leading $$ stripped on match)
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

### Tac Workers

Tac Workers let page and component scripts call a browser-local worker backend
with the same authoring feel as Yon routes:

```text
browser/workers/
  language/
    rust/
      tac.rs
    c/
      tac.c
    cpp/
      tac.cpp
    zig/
      tac.zig
    python/
      tac.py
    csharp/
      tac.cs
    go/
      tac.go
    javascript/
      tac.js
    typescript/
      tac.ts
```

The worker is shaped exactly like a Yon route handler - `impl Handler` with
methods named after HTTP verbs:

```rust
// browser/workers/language/rust/tac.rs
impl Handler {
    pub fn GET(request: Request) -> i32 { request.len() }
    pub fn POST(request: Request) -> String { /* ... */ }
    pub fn PATCH(request: Request) -> Json { json(request.body()) }
}
```

Page and component scripts invoke it with the native `fetch` API - the request
verb selects the handler method (default `GET`):

```js
export default class extends Tac {
  summary = ''

  async summarize(text) {
    const response = await fetch('tac://language/rust', { method: 'POST', body: { text } })
    const payload = await response.json()
    this.summary = payload.body.result
  }
}
```

At bundle time, Tachyon compiles each worker source into sibling runtime assets
under `dist/workers/**`:

```text
dist/workers/language/rust/tac.worker.js
dist/workers/language/rust/tac.wasm
dist/workers/language/c/tac.worker.js
dist/workers/language/c/tac.wasm
dist/workers/language/cpp/tac.worker.js
dist/workers/language/cpp/tac.wasm
dist/workers/language/zig/tac.worker.js
dist/workers/language/zig/tac.wasm
dist/workers/language/python/tac.worker.js
dist/workers/language/python/tac.wasm
dist/workers/language/csharp/tac.worker.js
dist/workers/language/csharp/tac.wasm
dist/workers/language/go/tac.worker.js
dist/workers/language/go/tac.wasm
dist/workers/language/javascript/tac.worker.js
dist/workers/language/javascript/tac.wasm
dist/workers/language/typescript/tac.worker.js
dist/workers/language/typescript/tac.wasm
```

`fetch('tac://language/rust', init)` resolves to the generated
`/workers/language/rust/tac.worker.js` module, runs the matching verb method in
a Web Worker, and returns a normal browser `Response`. Every non-`tac://` URL is
delegated to the platform `fetch`, so local-first caching is unaffected.

Tac caches one browser Worker per route by default. Heavy compute paths can opt
into a Tachyon-managed route pool without changing the worker source:

```js
await fetch('tac://language/rust?pool=4', { method: 'POST', body })
await fetch('tac://language/rust', {
  method: 'POST',
  headers: { 'X-Tac-Workers': '4' },
  body,
})
await fetch('tac://language/rust', {
  method: 'POST',
  tac: { poolSize: 4 },
  body,
})
```

Pool sizes are clamped between `1` and `16`. Tachyon reuses the route pool and
dispatches calls to the least-busy worker with round-robin tie breaking, which
keeps concurrent Wasm work off the UI thread and spreads bursts before Tachyon
adds true shared-memory Wasm threading.

Compilation is fully in-house - no `rustc`/`clang`/`emcc`. Tachyon parses the
handler subset directly and emits `tac.wasm` exposing the worker ABI
(`memory`, `alloc`, optional `dealloc`, `call`, `output_ptr`, `output_len`).
**Rust** (`tac.rs`), **C** (`tac.c`), **C++** (`tac.cpp`), **Zig**
(`tac.zig`), **Python** (`tac.py`), **C#** (`tac.cs`), **Go** (`tac.go`),
**JavaScript** (`tac.js`), and **TypeScript** (`tac.ts`) frontends ship today.
Swift and Kotlin remain supported by Yon backend routes, but they are not part
of the frontend Tac Worker Wasm compiler surface.

How the in-house compiler works:

1. Tachyon finds `browser/workers/**/tac.<language>` during `bun run bundle`.
2. The language-specific frontend tokenizes/parses only Tachyon's documented
   handler subset, not the full language.
3. That parser emits a shared handler AST: methods named after HTTP verbs,
   typed expressions, local bindings, loops, conditionals, and request helpers.
4. `tac-handler-codegen.js` type-checks the AST, rejects unsupported methods or
   type mismatches, and emits Wasm through Tachyon's own encoder.
5. The generated `tac.worker.js` loads `tac.wasm`, dispatches the fetch method
   to the matching handler method, and returns a normal `Response`.

Worker contracts may also live beside the source as `OPTIONS.schema.json`, just
like Yon backend routes:

```text
browser/workers/language/rust/
  tac.rs
  OPTIONS.schema.json
```

When present, Tachyon emits the schema to
`dist/workers/<route>/OPTIONS.schema.json`, embeds it in the generated worker
runtime, validates declared request sections before calling Wasm, and validates
the response body for the matching status code before resolving the `fetch`.
The browser worker validator follows Tachyon's strict CHEX-style contract
shape: string leaves are regex patterns, data values are coerced to strings for
matching, objects reject unknown properties, keys may be nullable with `?`,
arrays must contain exactly one scalar or object template, and record
descriptors are single-key objects whose key starts with `^`.

Worker methods can return i32-backed integer aliases, `bool`,
`String`/`string`/`str`, or `Json`/`json`. Boolean responses are emitted as
real JSON `true`/`false` values. String responses are JSON-escaped
automatically. Json responses are copied into `body.result` as raw JSON, so
`json(request.body())` can echo a JSON object or array as a real structured
value. `request.json()` returns the whole request envelope as raw JSON for
handlers that need the method/body metadata together.

Integer aliases are intentionally backed by the same signed 32-bit Wasm lane
today. Use them for familiar authoring syntax, not native-width overflow
semantics. Float/double primitives are not exposed yet; they need a dedicated
f64 arithmetic and JSON formatting path before Tachyon can claim production
support for them.

Supported worker subset by language:

- Rust: `impl Handler`, `pub fn VERB(request: Request) -> i8|i16|i32|u8|u16|u32|isize|usize|bool|String|Json`,
  `let`/`let mut`, assignment, arithmetic, comparisons, logical `! && ||`,
  `if/else` expressions, `while`, string literals + `+`, `request.len()`,
  `request.body()`, `request.json()`, and `json(...)`.
- C: top-level `VERB(Request request)` functions returning
  `char|short|int|unsigned int|bool|string|json` plus fixed-width
  `*_t` aliases, declarations, assignment, arithmetic, comparisons, logical
  `! && ||`, ternary `?:`, `while`, string literals + `+`, `request.len()`,
  `request.body()`, `request.json()`, and `json(...)`.
- C++: `class Handler` static methods returning integer aliases, `bool`,
  `string`, or `json`, local declarations, assignment, arithmetic, comparisons,
  logical `! && ||`, ternary `?:`, `while`, string literals + `+`,
  `request.len()`, `request.body()`, `request.json()`, and `json(...)`.
- Zig: `const Handler = struct`, `pub fn VERB(request: Request) i8|i16|i32|u8|u16|u32|isize|usize|bool|string|json`,
  `const`/`var`, assignment, arithmetic, comparisons, logical `! && ||`,
  ternary `?:`, `while`, string literals + `+`, `request.len()`,
  `request.body()`, `request.json()`, and `json(...)`.
- Python: `class Handler`, optional `@staticmethod`, `def VERB(request) ->
  int|bool|str|json`, assignments, arithmetic, comparisons, Python logical
  `not/and/or`, Python ternary expressions (`a if condition else b`), `while`,
  string literals + `+`, `request.len()`, `request.body()`, `request.json()`,
  and `json(...)`.
- C#: `class Handler` static methods returning integer aliases, `bool`,
  `string`/`String`, or `Json`, declarations, assignment, arithmetic,
  comparisons, logical `! && ||`, ternary `?:`, `while`, string literals + `+`,
  `request.len()`, `request.body()`, `request.json()`, and `json(...)`.
- Go: `func (Handler) VERB(request Request) int|int8|int16|int32|uint|uint8|uint16|uint32|uintptr|bool|string|json`,
  `var` declarations, assignment, arithmetic, comparisons, logical `! && ||`,
  Tac-Go ternary `?:`, `for condition` loops, string literals + `+`,
  `request.len()`, `request.body()`, `request.json()`, and `json(...)`.
- JavaScript: `class Handler` methods named after HTTP verbs, optional
  `static`, JSDoc `@returns {number|boolean|string|json}` when inference is not
  enough, `const`/`let`/`var`, assignment, arithmetic, comparisons, logical
  `! && ||`, ternary `?:`, `while`, string literals + `+`, `request.len()`,
  `request.body()`, `request.json()`, and `json(...)`.
- TypeScript: `class Handler` or `export default class Handler`, methods named
  after HTTP verbs with a `TacWorkerRequest` parameter and return annotations
  `number|boolean|string|Json|json`, typed locals, assignment, arithmetic,
  comparisons, logical `! && ||`, ternary `?:`, `while`, string literals + `+`,
  `request.len()`, `request.body()`, `request.json()`, and `json(...)`.

---

## API Docs

Yon exposes an OpenAPI 3.1 document at `/openapi.json` and a self-hosted Tachyon docs UI at `/api-docs`.

- route response schemas are derived from each route's `OPTIONS.schema.json` file
- request schemas can also flow into the OpenAPI document when you define `request`
- the docs page is rendered by Tachyon-owned HTML, CSS, and JavaScript instead of a third-party docs bundle
- the docs UI supports request authorization, operation filtering, deep links, cURL generation, and live "try it out" execution with response inspection

---

<details>
<summary><h2 style="display:inline">OpenTelemetry Storage</h2></summary>

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
- persisted FYLO documents under `.tachyon-otel/.collections/otel-spans/`
- one server span and one nested handler span for the request

### Consuming Telemetry From FYLO

The example app includes a Yon telemetry consumer at `/languages/javascript/telemetry`.

- it reads `otel-spans` from FYLO
- parses the stored `otlpJson` payload back into OTLP JSON `TracesData`
- returns a monitoring-friendly summary plus recent spans

That route is implemented in [examples/server/routes/languages/javascript/telemetry/GET/yon.js](examples/server/routes/languages/javascript/telemetry/GET/yon.js), and the example dashboard uses it to render a live telemetry panel.

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

</details>

### `$` and `$$` Field Persistence

`$`-prefixed instance fields are automatically persisted to `sessionStorage`.
`$$`-prefixed instance fields are automatically persisted to `localStorage`.

```js
export default class extends Tac {
  /** @type {number} */
  $count = 0         // sessionStorage
  /** @type {string} */
  $$theme = 'dark'   // localStorage

  increment() {
    this.$count += 1
  }
  toggle() {
    this.$$theme = this.$$theme === 'dark' ? 'light' : 'dark'
  }
}
```

The persistence key is generated from:

- the Tac module path
- the current page path or generated component instance identity
- the field name (including the `$` or `$$` prefix)

That makes the key stable across reloads and unique per persisted field instance.

### Local-First `fetch()`

Inside Tac page/component scripts only, `fetch()` is wrapped with a local-first strategy:

- all request methods are supported and forwarded through the Tac wrapper
- successful `GET` and `HEAD` responses are written to IndexedDB
- later `GET` and `HEAD` calls read from the cache first
- `cache: 'reload'` bypasses the cached read
- successful non-`GET` requests invalidate cached `GET` and `HEAD` entries for the same URL so later reads do not serve stale data

This does not override the global browser `fetch` outside Tac page/component execution.

The compiler-injected `fylo` browser client adds a document-level local mirror
for FYLO reads. Tachyon runs that local mirror behind a dedicated browser worker
so OPFS reads, writes, and query filtering stay off the UI thread. The worker
stores hydrated FYLO documents in OPFS when the browser supports it, falls back
safely when OPFS is unavailable, and keeps the older IndexedDB response cache as
a network fallback layer for plain fetches. This worker boundary is also the
future seam for a native FYLO Wasm core when the FYLO package exposes one.

```js
export default class extends Tac {
  users = []

  async refresh() {
    const result = await fylo.users.find(
      { role: 'eq.admin', order: 'email.asc', limit: 25 },
      { cache: 'network-first' }
    )
    this.users = (result.docs ?? []).map((entry) => entry.doc)
  }

  async save(user) {
    await fylo.users.create(user)
    await this.refresh()
  }
}
```

FYLO read cache policies are:

- `cache-first`: default; return locally mirrored `find`, `list`, or `get` data first when present, then hydrate from the network
- `network-first`: try the network first, write successful results into the local mirror, and fall back to local data when offline
- `reload`: bypass local reads and refresh local data from the network
- `no-store`: skip local reads/writes and use the network path only

Successful FYLO mutations (`create`, `put`, `patch`, and `del`) still require a
successful Yon `/_fylo` response before Tachyon updates the browser-local mirror.
Authenticated FYLO data is stored in a credential-scoped namespace so one user's
local documents are not reused for another user's credentials.

FYLO reads can also run in a sync-first style from Tac scripts:

```js
export default class extends Tac {
  users = []

  initUsers() {
    this.unsubscribeUsers = fylo.users.subscribe(
      { role: 'eq.admin', order: 'email.asc' },
      (result) => {
        this.users = (result.docs ?? []).map((entry) => entry.doc)
      },
      { cache: 'network-first' }
    )
  }
}
```

`subscribe(query, callback, options)` performs an initial `find()`, subscribes to
the browser-local FYLO mirror for same-tab mutations, opens
`/_fylo/api/events/stream` as a server-sent event stream for remote changes, and
re-runs the query after each local or remote event. If `EventSource` cannot be
used, such as when the wrapper is sending explicit Basic auth headers, the same
API falls back to polling `fylo.<collection>.events()` with the configured
`pollMs`.

On the Yon side, handler responses are also given an inferred content type now:

- JSON-looking output is served as `application/json`
- other string output falls back to `text/plain`

### Scoped Component CSS

`tac.css` is automatically wrapped with a component scope:

```css
@scope ([data-tac-scope="clicker"]) { ... }
```

That scope is applied to the generated wrapper around each component instance.

---

## Shared Frontend Files

- `browser/shared/assets/*` is served at `/shared/assets/*`
- `browser/shared/data/*` is served at `/shared/data/*`
- `browser/shared/scripts/imports.js` is the optional browser entry
- `browser/shared/styles/*` is available for imports from `imports.js`

If `imports.js` imports CSS, Tachyon emits `/imports.css` and links it from generated HTML shells.

The example app uses a local `imports.js` plus shared assets/data. Keep demo-only browser helpers out of published runtime code; shared production styles should live under `browser/shared/assets` or `browser/shared/styles`.

---

<details>
<summary><h2 style="display:inline">Build Output</h2></summary>

`tac.bundle` writes a static-ready `dist/` directory.

`yon.serve --no-bundle` or `YON_SKIP_BUNDLE=true` starts the server without
regenerating `dist/`, which is useful when another build pipeline owns frontend
output. For post-processing that should run after every Tachyon bundle, export a
`postBundle` hook from `tac.config.js`:

```js
export default {
  async postBundle({ distRoot }) {
    // patch distRoot/index.html, write runtime config, copy deployment assets, etc.
  }
}
```

Typical output:

```text
dist/
  index.html
  docs/index.html
  pages/tac.js
  pages/docs/tac.js
  components/clicker/tac.js
  modules/*.js
  shared/assets/*
  shared/data/*
  workers/language/rust/tac.worker.js
  workers/language/rust/tac.wasm
  workers/language/c/tac.worker.js
  workers/language/c/tac.wasm
  workers/language/cpp/tac.worker.js
  workers/language/cpp/tac.wasm
  workers/language/zig/tac.worker.js
  workers/language/zig/tac.wasm
  workers/language/python/tac.worker.js
  workers/language/python/tac.wasm
  workers/language/csharp/tac.worker.js
  workers/language/csharp/tac.wasm
  workers/language/go/tac.worker.js
  workers/language/go/tac.wasm
  workers/language/javascript/tac.worker.js
  workers/language/javascript/tac.wasm
  workers/language/typescript/tac.worker.js
  workers/language/typescript/tac.wasm
  fylo-local-worker.js
  spa-renderer.js
  imports.js
  imports.css
```

> **Notes:**
>
> - there is no `dist/layouts/` output
> - page shells are embedded into `spa-renderer.js`
> - static assets are emitted under `dist/shared/assets/`
> - the runtime now uses one app shell template and injects the HMR client only in development

</details>

---

## Commands

<table>
<tr><th align="left">Command</th><th align="left">Description</th></tr>
<tr><td><code>yon.serve</code></td><td>Detects <code>browser/</code> and <code>server/</code> contents, serves frontend, backend, or full-stack app</td></tr>
<tr><td><code>tac.bundle</code></td><td>Builds <code>dist/</code></td></tr>
<tr><td><code>tac.bundle --watch</code></td><td>Keeps <code>dist/</code> fresh</td></tr>
<tr><td><code>tac.preview</code></td><td>Serves <code>dist/</code></td></tr>
<tr><td><code>tac.preview --watch</code></td><td>Rebuilds and previews frontend output together</td></tr>
</table>

---

## Operations

Built-in endpoints:

<table>
<tr><th align="left">Endpoint</th><th align="left">Purpose</th></tr>
<tr><td><code>/health</code></td><td>Health check</td></tr>
<tr><td><code>/healthz</code></td><td>Health check (k8s convention)</td></tr>
<tr><td><code>/ready</code></td><td>Readiness check</td></tr>
<tr><td><code>/readyz</code></td><td>Readiness check (k8s convention)</td></tr>
</table>

Tachyon also supports:

- origin-aware CORS rejection before handler execution
- proxy-aware request context
- in-memory rate limiting
- middleware-provided distributed rate limiting
- cache headers for runtime assets, chunks, shared assets, and shared data
- document-request detection using browser navigation headers such as `Sec-Fetch-Dest` / `Sec-Fetch-Mode`, with `Accept: text/html` kept as a fallback

---

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

---

## Security

- security headers on all responses
- Bun password verification for hashed Basic Auth
- request body and parameter limits
- handler timeout enforcement
- JWT expiry rejection when decodable
- route request/response validation through `OPTIONS.schema.json`

---

## Production Notes

- prefer `YON_BASIC_AUTH_HASH`
- set explicit `YON_ALLOW_ORIGINS`
- keep `YON_DATA_BROWSER_ENABLED=false` unless the FYLO browser is protected by
  hashed Basic Auth, explicit origins, and shared rate limiting
- configure `YON_TRUST_PROXY` when behind nginx, Caddy, or Cloudflare
- use a shared rate limiter for multi-instance deployments
- validate the built frontend with `tac.preview` before deploy

---

## License

MIT
