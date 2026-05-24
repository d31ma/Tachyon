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
- Companion `*.js`, `*.ts`, `*.wasm`, source-backed Wasm (`*.as.ts`, `*.rs`, `*.c`, `*.go`, `*.zig`, `*.wat`), and `*.css` files beside templates
- OOP-style companion classes with `export default class extends Tac`
- Wasm-backed Tac companions through the `tac-wasm-json@1` ABI and generated adapters
- Automatic persistence for `$`-prefixed (sessionStorage) and `$$`-prefixed (localStorage) instance fields
- Local-first browser `fetch()` for Tac page/component scripts with IndexedDB-backed read caching and mutation-aware invalidation
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

`bun run serve` is shape-aware: `browser/` only bundles and serves the frontend, `server/` only serves backend routes, and apps with both folders run as a full-stack app on one port.

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

- reactive page state
- persisted `$` (sessionStorage) and `$$` (localStorage) fields
- local-first fetches
- frontend-only external SSE streaming with reactive Tac updates
- prebuilt Tac Wasm companions with source examples in WAT, AssemblyScript, Rust, C, Go, and Zig
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
YON_CONTENT_SECURITY_POLICY=default-src 'self'; script-src 'self' 'wasm-unsafe-eval'
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

Tachyon uses `@d31ma/fylo@26.21.6`, which is filesystem-first and uses the
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

`FYLO_INDEX_BACKEND=s3-client` is also passed through to FYLO when you want FYLO
to store index keys through Bun's S3 client. The old `s3-prefix`/LocalStack
configuration is intentionally rejected so stale deployment env cannot silently
fall back to a different backend.

</details>

---

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

Every language route also demonstrates FYLO access through the `fylo.exec`
machine interface bundled by `@d31ma/fylo`. The examples extend the existing
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
| `GET /languages/ruby` | `language-route-events` | `createCollection`, `putData`, `findDocs` |
| `GET /languages/php` | `language-route-events` | `createCollection`, `putData`, `findDocs` |
| `GET /languages/go` | `language-route-events` | `createCollection`, `rebuildCollection` |
| `GET /languages/csharp` | `items` | `schemaCurrent`, `schemaHistory` |
| `POST /languages/java` | `items` | `schemaInspect` |
| `DELETE /languages/dart` | `language-route-events` | `createCollection`, `importBulkData` |
| `PATCH /languages/rust` | `fylo-disposable-runs` | `createCollection`, `dropCollection` |

Rules:

- handler files live at `<METHOD>/yon.<ext>` inside the route directory, e.g. `GET/yon.js`, `POST/yon.ts`, or `PATCH/yon.rs`
- the parent directory name must be an uppercase HTTP method such as `GET` or `POST`
- the `OPTIONS.schema.json` schema file sits as a sibling of the method directories
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

Yon invokes pure function/class handlers directly for dynamic runtimes and
generates tiny build wrappers for compiled/static runtimes. No third-party
adapter dependency is added; compiled handlers use the language toolchain already
on the developer or deployment machine.

<table>
<tr><th align="left">Language</th><th align="left">Supported handler shape</th></tr>
<tr><td>JavaScript</td><td><code>export function handler(request)</code> / <code>default class Yon.handler()</code></td></tr>
<tr><td>TypeScript</td><td><code>export function handler(request)</code> / <code>default class Yon.handler()</code></td></tr>
<tr><td>Python</td><td><code>def handler(request)</code> / <code>class Yon.handler()</code></td></tr>
<tr><td>Ruby</td><td><code>def handler(request)</code> / <code>class Yon#handler</code></td></tr>
<tr><td>PHP</td><td><code>function handler($request)</code> / <code>class Yon::handler</code></td></tr>
<tr><td>Dart</td><td><code>handler(Map&lt;String, dynamic&gt; request)</code></td></tr>
<tr><td>Go</td><td><code>func Handler(request map[string]any) any</code></td></tr>
<tr><td>Java</td><td><code>Yon.handler(Map&lt;String, Object&gt; request)</code></td></tr>
<tr><td>C#</td><td><code>Yon.Handler(JsonElement request)</code></td></tr>
<tr><td>Rust</td><td><code>pub fn handler(request: &amp;JsonValue) -&gt; impl Display</code></td></tr>
</table>

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
- `<loop :for="...">`
- `<logic :if="...">`
- `<switch :value="...">` with `<case :when="...">` and `<case default>`
- `<my-component />`
- `<my-component lazy />`

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
- `this.emit(name, detail)`
- `this.inject(key, fallback?)`
- `this.provide(key, value)`
- `this.onMount(fn)`
- `this.rerender()`
- `this.isBrowser`
- `this.isServer`
- `this.props`

### Wasm Companions

Tac can also load prebuilt WebAssembly companions. The browser still receives a generated JavaScript adapter that extends `Tac`, so templates keep the same shape:

```html
<!-- browser/components/clicker/tac.html -->
<button @click="increment()">{label}: {clicks}</button>
```

Place the Wasm module and manifest beside the template:

```text
browser/components/clicker/
  tac.html
  tac.wasm
  tac.tac.json
```

Or give Tachyon source and let `bun serve` / `bun run bundle` compile it before generating the Tac adapter:

```text
browser/components/clicker/
  tac.html
  tac.rs
  tac.tac.json
```

Source-backed companions are selected before a sibling `tac.wasm`, so app authors can keep a checked-in fallback while local development still compiles from source when the compiler is available. If source compilation fails and a sibling `.wasm` exists, Tachyon logs a warning and uses the prebuilt fallback; without a fallback, the bundle fails with the compiler error.

`tac.tac.json` declares the stable Tac ABI:

```json
{
  "abi": "tac-wasm-json@1",
  "state": {
    "clicks": 0,
    "label": "Ready"
  },
  "methods": ["increment"]
}
```

The Wasm module must export:

- `memory`
- `alloc(size) -> ptr`
- `dealloc(ptr, len)`
- `init(ptr, len)`
- `call(methodPtr, methodLen, payloadPtr, payloadLen)`
- `output_ptr() -> ptr`
- `output_len() -> len`

`init` receives JSON shaped as `{ "props": { ... } }`. `call` receives the method name as JSON plus a payload shaped as `{ "args": [...], "props": { ... }, "state": { ... } }`. Both functions report their response through `output_ptr` / `output_len`; the response JSON can contain `{ "state": { ... }, "result": ..., "effects": [...] }`.

Supported effects:

- `{ "type": "emit", "name": "saved", "detail": { ... } }`
- `{ "type": "provide", "key": "theme", "value": "dark" }`
- `{ "type": "rerender" }`

Any language can participate by compiling to a `.wasm` module that follows this ABI. Tachyon keeps rendering, event binding, persistence, local-first fetch, and DOM access in the generated adapter rather than exposing the DOM directly to Wasm.

<details>
<summary><h3 style="display:inline">Compiler Support</h3></summary>

Tachyon currently knows how to compile these source companions when the matching compiler is installed on the app author's machine:

<table>
<tr><th align="left">Extension</th><th align="left">Compiler</th><th align="left">Install</th></tr>
<tr><td><code>tac.as.ts</code></td><td><code>asc</code></td><td><code>bun add -d assemblyscript</code></td></tr>
<tr><td><code>tac.rs</code></td><td><code>rustc</code></td><td>Install Rust + <code>rustup target add wasm32-unknown-unknown</code></td></tr>
<tr><td><code>tac.c</code></td><td><code>clang</code></td><td>Install LLVM/Clang with WebAssembly target support</td></tr>
<tr><td><code>tac.go</code></td><td><code>tinygo</code></td><td>Standard Go browser Wasm target uses a Go runtime shim, not the Tac ABI shape</td></tr>
<tr><td><code>tac.zig</code></td><td><code>zig</code></td><td></td></tr>
<tr><td><code>tac.wat</code></td><td><code>wat2wasm</code></td><td>Install WABT</td></tr>
</table>

Compiler path overrides are available for CI or non-standard installs:

<table>
<tr><th align="left">Variable</th><th align="left">Compiler</th></tr>
<tr><td><code>TACHYON_WASM_ASC</code></td><td>AssemblyScript</td></tr>
<tr><td><code>TACHYON_WASM_RUSTC</code></td><td>Rust</td></tr>
<tr><td><code>TACHYON_WASM_CLANG</code></td><td>C (Clang)</td></tr>
<tr><td><code>TACHYON_WASM_TINYGO</code></td><td>Go (TinyGo)</td></tr>
<tr><td><code>TACHYON_WASM_ZIG</code></td><td>Zig</td></tr>
<tr><td><code>TACHYON_WASM_WAT2WASM</code></td><td>WAT (wabt)</td></tr>
</table>

</details>

The checked-in examples under `examples/browser/components/wasm/` use real source-backed companion filenames plus sibling `.wasm` fallbacks, so the example app runs without requiring every language compiler to be installed:

- `clicker/tac.wat` for raw WebAssembly text
- `assemblyscript/tac.as.ts`
- `rust/tac.rs`
- `c/tac.c`
- `go/tac.go`
- `zig/tac.zig`

Tachyon intentionally treats language compilers as optional; plain `.wasm` works without adding a framework dependency. For strict CSP deployments, keep `script-src 'wasm-unsafe-eval'` in `YON_CONTENT_SECURITY_POLICY` so browsers can instantiate Wasm modules.

### Decorator Form

The same context, lifecycle, and event helpers are also exposed as Stage 3 decorators. They move the wiring out of the constructor and onto the field or method that owns the value. Companion scripts can use them as bare identifiers — the Tachyon compiler auto-imports them when it sees the `@<name>` syntax, so no `import` line is needed in user code.

For editor and `checkJs` support in consuming apps, include Tachyon's ambient
globals once in the app:

```ts
/// <reference types="@d31ma/tachyon/globals" />
```

The `yon.init` scaffold writes this to `tachyon-env.d.ts` automatically. It
lets app-authored page and component scripts use bare `Tac`, `inject`,
`provide`, `env`, `onMount`, `emit`, `render`, and `fylo` without local imports
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

<table>
<tr><th align="left">Decorator</th><th align="left">Kind</th><th align="left">Behavior</th></tr>
<tr><td><code>@inject(key, fallback?)</code></td><td>field</td><td>Field initialized from <code>tac.inject(key, fallback)</code></td></tr>
<tr><td><code>@provide(key)</code></td><td>field</td><td>Initial value registered with <code>tac.provide(key, value)</code> after construction</td></tr>
<tr><td><code>@env(key, fallback?)</code></td><td>field</td><td>Field initialized from <code>tac.env(key, fallback)</code></td></tr>
<tr><td><code>@onMount</code></td><td>method</td><td>Method registered as an <code>onMount</code> handler bound to the instance</td></tr>
<tr><td><code>@emit(name)</code></td><td>method</td><td>Return value (or resolved value for async) emitted as <code>name</code>. Rejections propagate without emitting.</td></tr>
</table>

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

`$`-prefixed and `$$`-prefixed persistent fields are reactive too, and still write through to `sessionStorage` / `localStorage`. `this.rerender()` remains available for rare cases where code mutates nested object/array contents in place instead of assigning a new field value.

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
- configure `YON_TRUST_PROXY` when behind nginx, Caddy, or Cloudflare
- use a shared rate limiter for multi-instance deployments
- validate the built frontend with `tac.preview` before deploy

---

## License

MIT
