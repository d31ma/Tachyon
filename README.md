<p align="center">
  <a href="https://github.com/d31ma/Tachyon/releases/latest"><img src="https://img.shields.io/github/v/release/d31ma/Tachyon?style=flat&label=release" alt="latest release"></a>
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
- OOP-style companion classes with `export default class`; Tachyon injects the Tac base class during bundling
- Browser-local Tac Workers compiled in-house to `tac.wasm` (no external toolchain), invoked with `fetch("tac://...")`
- Automatic persistence for `$`-prefixed (sessionStorage) and `$$`-prefixed (localStorage) instance fields
- Local-first browser `fetch()` plus worker-owned OPFS-backed FYLO document mirrors for Tac page/component scripts
- Explicit browser env allowlisting through `TAC_PUBLIC_ENV` and `this.env(...)`
- Static export with prerendered `dist/**/index.html`
- Platform-aware Tac globals for `web`, desktop (`macos`, `windows`, `linux`), and mobile (`ios`, `android`) targets
- Shared frontend assets under `/shared/assets/*`
- Shared frontend data under `/shared/data/*`
- Generated OpenAPI 3.1 docs at `/openapi.json` with a self-hosted Tachyon docs UI at `/api-docs`
- FYLO-backed OpenTelemetry storage with request and handler span correlation
- Built-in health/readiness endpoints
- Proxy-aware request context, CORS enforcement, and optional rate limiting

---

## Install

Tachyon ships as a single standalone binary — `ty` — with no npm package and no
Bun required on your machine. The installer also pulls the `fylo` (document
store), `chex` (schema validation), and `ttid` (identifier generation) binaries
Tachyon drives at runtime.

```bash
# macOS / Linux
curl -fsSL https://tachyon.del.ma/install.sh | sh
```

```powershell
# Windows (PowerShell)
irm https://tachyon.del.ma/install.ps1 | iex
```

The installer grabs the right binary for your OS/arch from the latest
[GitHub release](https://github.com/d31ma/Tachyon/releases), verifies its
checksum, shows a staged Tachyon progress bar, and puts `ty` on your PATH.
Verify with `ty --version`.

---

## Quick Start

```bash
ty init my-app
cd my-app
ty serve
```

`ty init` prompts for an app name when run interactively. For scripts, pass it
explicitly:

```bash
ty init my-app --name "My App"
```

Useful commands:

<table>
<tr><th align="left">Command</th><th align="left">Description</th></tr>
<tr><td><code>ty serve</code></td><td>Shape-aware dev server — frontend, backend, or full-stack on one port</td></tr>
<tr><td><code>ty bundle</code></td><td>Build static <code>dist/</code> output</td></tr>
<tr><td><code>ty preview</code></td><td>Serve the built <code>dist/web</code> directory by default</td></tr>
<tr><td><code>ty cache clean</code></td><td>Clear the standalone binary's materialized runtime cache</td></tr>
</table>

`ty serve` is shape-aware: `client/` only bundles and serves the frontend,
`server/` only serves backend routes, apps with both folders run as a full-stack
app on one port, and `client/` + `db/` apps mount Tachyon's built-in FYLO
browser routes when `YON_DATA_BROWSER_ENABLED=true`.

### Standalone Runtime Cache

Live `Bun.build()` artifacts stay in memory while `ty serve` runs. The
standalone `ty` binary stores only its materialized framework source files in a
versioned OS cache so Bun can resolve them during a build: `~/Library/Caches/Tachyon`
on macOS, `%LOCALAPPDATA%\\Tachyon\\Cache` on Windows, and
`$XDG_CACHE_HOME/tachyon` (or `~/.cache/tachyon`) on Linux. Set
`TACHYON_CACHE_DIR` to override the cache root, inspect it with `ty cache`, and
remove only this runtime cache with `ty cache clean`.

---

## Scaffold Layout

```text
client/
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
    posts/
      yon.js
  data/
  deps/
```

Scaffolds are JavaScript-first and use strict JSDoc rather than TypeScript source files.
The runtime still supports TypeScript companion scripts when you want them.

The website app in [website/](website/) is the canonical Tac showcase. It is
frontend-only: Tac Workers mimic the backend and the in-browser FYLO client
(OPFS) mimics the database, so the same static bundle works on every target:

- a guided capability atlas joining native HTML/CSS/JavaScript surfaces with
  working Tac, worker, and FYLO flows rather than isolated code snippets
- reactive page state
- accessible native controls and a reactive canvas studio with semantic
  `progress`, `meter`, `output`, `time`, and `details` elements
- persisted `$` (sessionStorage) and `$$` (localStorage) fields
- local-first fetches and OPFS-backed FYLO document collections with CRUD,
  cache policies, and live same-tab subscriptions
- browser-local Tac Workers answering every HTTP verb from `tac.wasm`
- frontend-only external SSE streaming and tab-to-tab realtime with durable,
  OPFS-replayed history
- client-side telemetry spans stored as FYLO documents
- shared data, shared assets, a browser entry, and component companions

The full-stack backend showcase — polyglot handlers in multiple languages,
MVC routing, server FYLO, realtime mailboxes, OpenAPI docs, middleware, and
OpenTelemetry — lives in [tests/fixtures/fullstack/](tests/fixtures/fullstack/),
where the integration suite exercises it end-to-end.

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
TAC_DANGEROUS_CAPABILITIES=
TAC_NATIVE_CAPABILITIES=

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
YON_PAGES_PATH=client/pages
YON_COMPONENTS_PATH=client/components
YON_WORKERS_PATH=client/workers
YON_ASSETS_PATH=client/shared/assets
YON_SHARED_SCRIPTS_PATH=client/shared/scripts
YON_SHARED_STYLES_PATH=client/shared/styles
YON_SHARED_DATA_PATH=client/shared/data
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

Tachyon drives the FYLO GitHub release binary through its vendored runtime shim.
FYLO is filesystem-first and uses the `local-fs` index backend by default. Set
`FYLO_ROOT` to the directory that should contain FYLO-managed collections:

```env
FYLO_ROOT=db
FYLO_SCHEMA_DIR=db/schemas
FYLO_INDEX_BACKEND=local-fs
```

Run the full-stack fixture app with its normal environment:

```bash
cd tests/fixtures/fullstack
ty serve
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
    language/                     -> consolidated polyglot showcase (one route, several languages)
      yon.js                      -> GET, HEAD               /language          (route map; JavaScript)
      yon.rs                      -> POST, DELETE            /language          (Rust; generated YonJson adapter)
      yon.cpp                     -> PUT, PATCH              /language          (C++; generated YonJson adapter)
      javascript/yon.js           -> GET, POST, PUT          /language/javascript        (status-code diagnostics)
      python/yon.py               -> GET                     /language/python
      rust/yon.rs                 -> GET                     /language/rust
      items/yon.ts                -> GET, POST, DELETE       /language/items             (RESTful CRUD, TypeScript)
      items/_id/yon.ts            -> GET, PUT, PATCH, DELETE /language/items/:id
      versions/_version/yon.py    -> GET, PATCH, DELETE      /language/versions/:version (dynamic path param, Python)
      fylo/yon.js                 -> POST                    /language/fylo              (FYLO machine interface)
      telemetry/yon.js            -> GET                     /language/telemetry         (OTLP trace summary)
      OPTIONS.schema.json         -> per-directory route schema files
    realtime/                     -> SSE realtime demo (clients, messages)
  services/                       -> application/business logic
  repositories/                   -> database and persistence access
  data/                           -> local example data
```

The full-stack fixture app intentionally uses an MVC-style backend dependency direction:
`routes -> services -> repositories`. Route files should stay small and call a
service. Services coordinate validation, business rules, and multiple
dependencies. Repositories are the only layer that talks directly to persistence
or runtime data sources. The `/language` route is the single backend showcase:
one polyglot route whose verbs are split across JavaScript, Rust, and C++
handlers, plus sub-routes for CRUD items, a dynamic path-param route, the FYLO
machine interface, and a telemetry consumer.

The `/language` verbs are split across three languages on the same route —
`GET`/`HEAD` (JavaScript), `POST`/`DELETE` (Rust), and `PUT`/`PATCH` (C++) — to
demonstrate the polyglot handler model. The Rust and C++ handlers use the
generated dependency-free `YonJson` adapter; each route runs as a process, and
compiled-language handlers compile on first request and cache the result — the
`server/` source ships as-is, with no separate build step or binary-only
deployment. The in-house WASM compiler now serves Tac frontend workers only.

Yon imposes no list of supported languages. A `yon.<ext>` handler runs by its
extension: `.go` runs via `go run`, `.rb` via `ruby`, and so on — a default
`interpreters` map covers common languages and is extensible/overridable under
`interpreters` in `.tachyonrc`. No shebang or `chmod` required: write
`server/routes/ping/yon.go`, declare its HTTP methods in an adjacent
`OPTIONS.schema.json`, and it runs as a process that reads the JSON request on
stdin and writes the response on stdout. (A prebuilt executable or a script
with its own shebang also works.) The built-in languages (JS/TS, Python, Ruby,
PHP, and the compiled ones) are conveniences that let you write a
`class Handler` and skip the stdin/stdout glue.

`POST /language/fylo` drives the FYLO machine interface end-to-end through
`fylo exec`: the route calls a service, the service calls a repository or
language-native process helper, and that layer sends JSON operations to the
installed FYLO binary. Set `FYLO_EXEC_PATH=/path/to/fylo` or
`FYLO_BINARY=/path/to/fylo` to point helpers at a pinned executable.

| Route | Demonstrates |
| ----- | ------------ |
| `GET, HEAD /language` | Route map and the polyglot method split (JavaScript) |
| `POST, DELETE /language` | Echo / size-confirm handlers (Rust) |
| `PUT, PATCH /language` | Echo / patch-summary handlers (C++) |
| `GET, POST, PUT /language/javascript` | Status-code diagnostics and echo responses |
| `GET /language/python`, `GET /language/rust` | Per-language GET handlers |
| `GET, POST, DELETE /language/items`, `…/items/:id` | RESTful CRUD (TypeScript) |
| `GET, PATCH, DELETE /language/versions/:version` | Dynamic path-param route (Python) |
| `POST /language/fylo` | Full FYLO machine-interface showcase (JavaScript) |
| `GET /language/telemetry` | OTLP trace-summary consumer (JavaScript) |

Rules:

- handler files live at `<route>/yon.<ext>` and export or define `class Handler`
- HTTP verbs are public static methods such as `GET`, `POST`, `PUT`, and `DELETE`
- the `OPTIONS.schema.json` schema file sits beside the route handler file
- dynamic route segments use `_slug` on disk and become `:slug` at runtime
- the first segment cannot be dynamic
- adjacent dynamic segments are not allowed

A route file at `<route>/yon.<ext>` defines a `class Handler` whose static
methods are named after HTTP verbs. The route path is the directory path, so
`server/routes/items/yon.ts` serves `/items`, and each static method handles the
verb it is named for — `GET`, `POST`, `PUT`, `DELETE`, `PATCH`, `HEAD`, or
`OPTIONS`.

```ts
// server/routes/items/yon.ts  ->  /items
export class Handler {
  static GET() {
    return { items: [] }
  }

  static async POST(request: { body?: unknown }) {
    return { created: request.body }
  }
}
```

A single route can be split across languages: add sibling `yon.<ext>` files that
declare non-overlapping methods — for example a `yon.ts` serving `GET`/`POST`
beside a `yon.rs` serving `PUT`/`DELETE`.

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

When `NODE_ENV=production`, Yon caches compiled Java, C#, Dart, C++, and Rust
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
<tr><td>Rust</td><td><code>struct Handler; impl Handler { pub fn GET(request: &amp;YonJson) -&gt; YonJson {} }</code></td></tr>
</table>

Java, C++, and Rust intentionally stay dependency-free. Yon generates a tiny JSON
adapter beside the compiled wrapper:

- Java receives a `java.util.Map<String, Object>` or `Object`.
- C++ receives a generated `YonJson` helper with object lookup, scalar coercion,
  and JSON serialization.
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
style used by the full-stack fixture app.

---

## Frontend Routing

Tac page routes live in `client/pages`.

```text
client/pages/
  tac.html             -> /
  docs/
    tac.html           -> /docs
  blog/
    _slug/
      tac.html         -> /blog/:slug
```

If an ancestor page contains `<slot />`, it acts as a reusable shell for descendant pages.

Tac components live in `client/components`.

```text
client/components/
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
client/components/clicker/tac.html       -> <clicker />
client/components/panel/users/tac.html   -> <panel-users />
```

Flat templates and hyphenated folder names such as
`client/components/clicker.html` and `client/components/panel-users/tac.html`
are rejected so app structure, generated module paths, CSS scopes, and template
tags all use the same naming rule.

---

## Tac Templates

Templates support:

- `{expr}` for escaped interpolation
- `{!expr}` for trusted raw HTML
- `on:<event>="handler()"` for event binding
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

#### Literal braces in prose (`\{` / `\}`)

`{ ... }` in text content is evaluated as JavaScript, so documentation prose
that contains braces — e.g. `a JSON array of { title, value } nodes` — would be
run as an expression and throw `ReferenceError: title is not defined` during
prerender. Escape literal braces with a backslash:

```html
<p>a JSON array of \{ title, value, children \} nodes</p>
```

`\{` and `\}` render as literal `{` and `}` and are never interpolated. They mix
freely with real interpolation: `{ count } items in \{ a set \}`.

#### JSON in attribute values

Attribute values are emitted verbatim (no `{ }` interpolation), so embedded JSON
survives as-is — write it with single outer quotes and literal double quotes:

```html
<x-el data='[{"title":"x"}]'></x-el>
```

The compiler entity-escapes the inner `"` to `&quot;` when re-serializing, so the
value round-trips intact in the DOM. Multiline values are fine. (Pre-existing
entities such as `&#39;` are preserved untouched — values are read raw.)

#### Event handlers (`on:<event>`)

Bind events with an `on:<event>` attribute. The value must be a **single
expression** — typically a method call:

```html
<button on:click="refresh()">Refresh</button>
<w-data-table on:update:selected="select($event)" />
```

- The event object is available as **`$event`** (alias `__event__`).
- Multi-statement handlers and bare blocks are not supported; move that logic
  into a companion-script method and call it (`on:load="loadMore($event)"`). A
  non-expression value fails at build time with a clear, attribute-named error.
- Any event name works, including custom and colon names (`on:save`,
  `on:update:selected`) — there is no allow-list of known DOM events.
- **The `on:` prefix is what marks an event.** A plain attribute or component
  prop that merely starts with `on` (e.g. `onboarding`, `online`) is left alone
  — only the `on:` namespace is compiled to a handler. A literal native
  `onclick="…"` (no colon) likewise passes through untouched.

**Web-component interop.** Tac never leaves an `on:<event>` binding on the live
DOM. At compile time it becomes a `data-tac-on-<event>` marker that Tac's
delegation keys off (colons in event names are encoded as `__`), so the browser
never executes it as a native global-scope handler — the expression resolves in
companion scope and triggers a rerender. Light-DOM component libraries (e.g.
DuVay) that emit standard bubbling/composed `CustomEvent`s work out of the box
and won't double-bind Tac's generated handler — no Shadow DOM or Tac-specific
event API required.

**How delegation works (and why it's cheap).** Tac borrows the best of React,
Svelte 5, Solid, Vue, and Angular and goes a step further:

- **One listener per event type, at `document`.** Like React/Solid/Svelte, Tac
  delegates rather than attaching a listener to every element — so the listener
  count is bounded by the number of distinct event *types*, not by the number of
  elements or handlers.
- **Compile-time event set, registered once.** The compiler already knows every
  `on:<event>` in a module, so it emits that set and the runtime registers the
  listeners a single time when the module first renders. Tac never scans the DOM
  to discover handlers — there is **zero per-render event-wiring work** (no
  per-element attach/detach on updates the way Vue/Svelte/Solid do on create).
- **`composedPath()` dispatch.** On an event, Tac walks the composed path once
  (Solid/Svelte-style) to find the nearest element carrying the matching marker
  — fast, and correct for `composed` events that cross shadow boundaries.
- **No synthetic event objects.** Unlike React, Tac dispatches the native event
  directly; there is nothing to allocate or pool per event.
- **Deferred, replay-safe hydration (on by default).** Registering the delegated
  listeners is held off the critical path until the browser is idle
  (`requestIdleCallback`) — or until the user first interacts, whichever comes
  first (Astro-style intent hydration). To make that lossless, a tiny inline
  script in the document `<head>` runs before the runtime and **captures**
  click/submit interactions that land on a Tac handler during the pre-hydration
  "dead zone", neutralizing their default. The moment delegation is registered,
  those interactions are **replayed** through it — so a click on a button or link
  made before the page finished loading still does the right thing (Angular ships
  this behind `withEventReplay()`; in Tac it's the default).

#### Re-render scope (component-scoped by default)

A handler or state change inside a component re-renders **only that component's
subtree**, not the whole page — so update cost scales with the component, not the
page size. This is automatic; components are the re-render boundary.

- **Cross-component updates go through `publish` / `subscribe`.** A component does
  not implicitly re-render content owned by its parent or siblings; publish a
  signal and let the other component subscribe (its own subtree then re-renders).
- **Page-level state** (a companion on the page root) still re-renders the page.
- **Escape hatch:** call `rerender({ global: true })` to force a full-page
  re-render for the rare case a component must refresh content outside itself.
  Plain `rerender()` stays scoped to the calling component.
- **Looped/repeated components** fall back to a full re-render of their container
  (they share an internal id space, so isolated re-render isn't safe) — correct,
  just not individually optimized.

Example page:

```html
<!-- client/pages/tac.html -->
<section class="hero">
  <h1>{headline}</h1>
  <p>{subtitle}</p>
  <button on:click="refresh()">Refresh</button>
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
// client/pages/tac.js
const Status = {
  Loading: 'loading',
  Pending: 'pending',
  Ready: 'ready'
}

export default class {
  /** @type {number} */
  $visits = 0
  Status = Status
  status = Status.Loading
  /** @type {string} */
  headline = 'Tac + Yon'
  /** @type {string} */
  subtitle = 'Reactive frontend, polyglot backend.'

  constructor() {
    this.$visits += 1
    if (typeof document !== 'undefined') document.title = 'Home'
  }

  async refresh() {
    const response = await this.fetch('/language/javascript')
    const payload = await response.json()
    this.subtitle = String(payload.message ?? this.subtitle)
  }
}
```

Anonymous companion classes are fully supported and do not need to write `extends Tac`:

```js
export default class {}
```

---

## Tac Companion Scripts

Companion scripts are instantiated automatically during render. Their fields and methods are visible in the matching HTML template without the developer manually referencing the class instance.

For JavaScript and TypeScript page/component companions, write `export default class { ... }`.
During bundling, Tachyon injects the Tac base class for default-exported
companion classes that do not already declare an `extends` clause.

Companion authors only need to think about their class state and methods.
Internal runtime helper plumbing is attached by the framework and does not need
to be imported, typed, or threaded through user code.

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

For editor and `checkJs` support, the `ty init` scaffold writes Tachyon's
ambient globals into a self-contained `tachyon-env.d.ts` in the app root — no
package reference required, since Tachyon ships as a binary rather than an npm
module. That declaration lets app-authored page and component scripts use bare
`Tac`, `publish`, `subscribe`, `env`, `onMount`, `fylo`, and `Worker` without
local imports or `Cannot find name` diagnostics from TypeScript-aware tooling.

If the app also uses plain ESLint `no-undef`, declare the same identifiers as
`readonly` globals in the app's flat config:

```js
export default [{
  files: ['client/**/*.{js,ts}'],
  languageOptions: {
    globals: { Tac: 'readonly', publish: 'readonly', subscribe: 'readonly', env: 'readonly', onMount: 'readonly', fylo: 'readonly' }
  }
}]
```

```js
export default class {
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
  async save(payload) { return await this.fetch('/language/items', { method: 'POST', body: JSON.stringify(payload) }) }
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

The decorators are only available inside companion scripts, where the Tachyon
compiler injects them automatically when it sees the `@<name>` syntax — there is
no module to import, since Tachyon ships as the `ty` binary rather than an npm
package.

### Reactive Fields

Tac companion fields are reactive in the browser. Assigning to a declared instance field schedules one batched rerender automatically, so app code does not need to call `this.rerender()` after normal state changes:

```js
export default class {
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
export default class {
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
export default class {
  apiBase = this.env('PUBLIC_API_BASE_URL', '/language/javascript')
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
client/workers/
  language/
    rust/
      tac.rs
    javascript/
      tac.js
    typescript/
      tac.ts
    apple/
      tac.swift
    android/
      tac.kt
    windows/
      tac.cs
```

The worker is shaped exactly like a Yon route handler - `impl Handler` with
methods named after HTTP verbs:

```rust
// client/workers/language/rust/tac.rs
impl Handler {
    pub fn GET(request: Request) -> i32 { request.len() }
    pub fn POST(request: Request) -> String { /* ... */ }
    pub fn PATCH(request: Request) -> Json { json(request.body()) }
}
```

Page and component scripts invoke it with the native `fetch` API - the request
verb selects the handler method (default `GET`):

```js
export default class {
  summary = ''

  async summarize(text) {
    const response = await fetch('tac://language/rust', { method: 'POST', body: { text } })
    const payload = await response.json()
    this.summary = payload.body.result
  }
}
```

At bundle time, Tachyon compiles each worker source into sibling runtime assets
under each selected target directory. For the default web target, that means
`dist/web/workers/**`:

```text
dist/web/workers/language/rust/tac.worker.js
dist/web/workers/language/rust/rs.wasm
dist/web/workers/language/javascript/tac.worker.js
dist/web/workers/language/javascript/js.wasm
dist/web/workers/language/typescript/tac.worker.js
dist/web/workers/language/typescript/ts.wasm
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

Browser worker compilation is fully in-house. Tachyon parses the supported
handler subset directly and emits Wasm exposing the worker ABI
(`memory`, `alloc`, optional `dealloc`, `call`, `output_ptr`, `output_len`).
Tac supports five worker language families with strict target scopes:

- **Rust** (`tac.rs`): web, macOS, Windows, Linux, iOS, and Android
- **JavaScript/TypeScript** (`tac.js`, `tac.ts`): web
- **Swift** (`tac.swift`): macOS and iOS
- **Kotlin** (`tac.kt`): Android
- **C#** (`tac.cs`): Windows

Native WebView bundles may use the browser/Wasm path for web-scoped workers.
Files whose language does not support the selected target are not bundled.

How the in-house compiler works:

1. Tachyon finds `client/workers/**/tac.<language>` during `ty bundle`.
2. The applicable language frontend tokenizes/parses only Tachyon's documented
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
client/workers/language/rust/
  tac.rs
  OPTIONS.schema.json
```

When present, Tachyon emits the schema to
`dist/<target>/workers/<route>/OPTIONS.schema.json`, embeds it in the generated
worker runtime, validates declared request sections before calling Wasm, and
validates the response body for the matching status code before resolving the
`fetch`.
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

Handlers can read request fields by key with `request.query("k")`,
`request.path("k")`, and `request.header("k")` (each returns a `String`, empty
when the key is absent). These resolve through a host-provided runtime import —
the host does the JSON lookup, so the wasm carries no JSON parser. A JSON object
literal `{ "key": value, ... }` builds a structured `Json` response directly to
JSON text (values rendered by type: integers as numbers, `bool` as
`true`/`false`, `String` quoted and escaped, `Json` embedded raw), so wasm
routes can return validated object responses without a JSON library inside the
module.

Tac Workers also expose a deliberately small platform boundary through
`request.platform("key")`. This is the only OS-like surface available to
browser-local Wasm workers; page and component scripts do not get a separate
Tac OS API. The generated worker host resolves curated, non-secret facts such as
`"os"`, `"arch"`, `"runtime"`, `"target"`, `"targets"`, `"cpuCores"`,
`"language"`, `"timezone"`, `"online"`, and `"touch"`. In browsers this is
backed by standard Web Worker globals and therefore works across macOS,
Windows, Linux, Android, and iOS within each browser's sandbox. It does not
grant arbitrary filesystem, process, device, or network system calls.

```rust
impl Handler {
    pub fn GET(request: Request) -> String {
        "running on " + request.platform("os")
    }
}
```

For native OS capabilities, Tac workers use a fail-closed bridge modeled after
Electron/Tauri's privileged-host pattern. A worker does not receive raw OS
access. Instead it returns a `$tacNative` envelope, and the generated worker
runtime brokers that request to the page/native host only if the capability is
listed in the `TAC_NATIVE_CAPABILITIES` environment variable at compile time.

```bash
TAC_NATIVE_CAPABILITIES=app.info,clipboard.readText
```

```rust
impl Handler {
    pub fn PATCH(request: Request) -> Json {
        json(request.body())
    }
}
```

```js
await fetch('tac://native', {
  method: 'PATCH',
  body: JSON.stringify({
    $tacNative: {
      capability: 'app.info',
      payload: {}
    }
  })
})
```

The brokered capability set includes `app.info`, `clipboard.readText`,
`clipboard.writeText`, `openUrl`, and `file.openText`. Browser builds use
web-platform fallbacks where available; native WebView builds can route the
same request through `window.__tcNativeBridge__.invoke(...)`.

Raw OS capabilities (`fs.*`, `shell.*`, `process.*`) require two independent
authorizations at compile time: they must appear in both
`TAC_NATIVE_CAPABILITIES` and `TAC_DANGEROUS_CAPABILITIES`:

```bash
TAC_NATIVE_CAPABILITIES=fs.readText
TAC_DANGEROUS_CAPABILITIES=fs.readText
```

A raw OS capability invoked without both allowlist entries is rejected at runtime.
Both lists are resolved at compile time and baked into the worker, since browser
workers have no `process.env`.

Initial raw capabilities are `fs.readText`, `fs.writeText`, `fs.readDir`, and
`shell.exec`. They are intended for trusted native hosts only. The browser
sandbox cannot execute these directly; it must either run in a host that exposes
Bun/Node-style OS APIs or delegate through a native WebView bridge. Raw sockets,
secrets, and process management beyond `shell.exec` remain disabled.
`shell.exec` accepts `{ "command": "...", "args": ["..."], "cwd": "..." }`
and returns `{ command, args, cwd, exitCode, stdout, stderr }`.

Integer aliases are intentionally backed by the same signed 32-bit Wasm lane
today. Use them for familiar authoring syntax, not native-width overflow
semantics. Float/double primitives are not exposed yet; they need a dedicated
f64 arithmetic and JSON formatting path before Tachyon can claim production
support for them.

Supported in-house Wasm subset by language:

- Rust: `impl Handler`, `pub fn VERB(request: Request) -> i8|i16|i32|u8|u16|u32|isize|usize|bool|String|Json`,
  `let`/`let mut`, assignment, arithmetic, comparisons, logical `! && ||`,
  `if/else` expressions, `while`, string literals + `+`, `request.len()`,
  `request.body()`, `request.json()`, and `json(...)`.
- C#: `class Handler` static methods returning integer aliases, `bool`,
  `string`/`String`, or `Json`, declarations, assignment, arithmetic,
  comparisons, logical `! && ||`, ternary `?:`, `while`, string literals + `+`,
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
cd tests/fixtures/fullstack
YON_OTEL_ENABLED=true \
YON_OTEL_ROOT=.tachyon-otel \
YON_OTEL_SERVICE_NAME=tachyon-dev \
YON_BASIC_AUTH_HASH="$(bun -e "console.log(await Bun.password.hash('admin:pass'))")" \
bun ../../../src/cli/serve.js
```

Then send a traced request:

```bash
curl -i \
  -H 'Authorization: Basic YWRtaW46cGFzcw==' \
  -H 'X-Request-Id: manual-otel-test' \
  -H 'traceparent: 00-0123456789abcdef0123456789abcdef-1111111111111111-01' \
  http://127.0.0.1:8000/language/javascript
```

You should see:

- `Traceparent` and `X-Trace-Id` in the response headers
- persisted FYLO documents under `.tachyon-otel/.collections/otel-spans/`
- one server span and one nested handler span for the request

### Consuming Telemetry From FYLO

The full-stack fixture app includes a Yon telemetry consumer at `/language/telemetry`.

- it reads `otel-spans` from FYLO
- parses the stored `otlpJson` payload back into OTLP JSON `TracesData`
- returns a monitoring-friendly summary plus recent spans

That route is implemented in [tests/fixtures/fullstack/server/routes/language/telemetry/yon.js](tests/fixtures/fullstack/server/routes/language/telemetry/yon.js).

The fixture also includes a tiny alerting worker at [tests/fixtures/fullstack/server/workers/telemetry-alert-worker.js](tests/fixtures/fullstack/server/workers/telemetry-alert-worker.js).

Run it against the fixture app with:

```bash
cd tests/fixtures/fullstack
YON_TELEMETRY_URL=http://127.0.0.1:8000/language/telemetry?limit=25 \
YON_BASIC_AUTH_HEADER='Basic YWRtaW46cGFzcw==' \
YON_ALERT_SLOW_MS=500 \
YON_ALERT_STATUS_CODE=500 \
bun ./server/workers/telemetry-alert-worker.js
```

It polls the telemetry endpoint, flags slow routes and server errors, and prints structured JSON that can be shipped to another service or cron job.

</details>

### `$` and `$$` Field Persistence

`$`-prefixed instance fields are automatically persisted to `sessionStorage`.
`$$`-prefixed instance fields are automatically persisted to `localStorage`.

```js
export default class {
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

The compiler-injected `fylo` browser client is backed by Tachyon's vendored FYLO
browser shim. FYLO stores browser-local documents in OPFS when the browser
supports it, falls back to memory when OPFS is unavailable, and prefers a
`SharedWorker`/`Worker` boundary so reads, writes, query filtering, and
collection events stay off the UI thread. Tachyon keeps the older IndexedDB
response cache as the network fallback layer for plain `fetch()` calls.

```js
export default class {
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

FYLO mutations update the browser-local mirror on a best-effort basis and then
sync through Yon's `/_fylo` boundary when available:

- `create`, `put`, `patch`, and `del` cover single-document writes.
- `batchPut`, `patchMany`, and `deleteMany` call FYLO's machine protocol for bulk operations.
- `restore`, `latest`, `inspect`, and `rebuild` expose FYLO maintenance and history helpers.
- `createCollection` and `dropCollection` are available on both `fylo.<collection>` and the root `fylo` object.

Authenticated FYLO data is stored in a credential-scoped namespace so one user's
local documents are not reused for another user's credentials.

```js
export default class {
  async importUsers(rows) {
    await fylo.createCollection('users')
    await fylo.users.batchPut(rows)
    await fylo.users.patchMany({
      query: { role: 'eq.admin' },
      patch: { reviewed: true }
    })
  }

  async recoverUser(id) {
    await fylo.users.restore(id)
    return fylo.users.latest(id)
  }
}
```

FYLO reads can also run in a sync-first style from Tac scripts:

```js
export default class {
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

- `client/shared/assets/*` is served at `/shared/assets/*`
- `client/shared/data/*` is served at `/shared/data/*`
- `client/shared/scripts/imports.js` is the optional browser entry
- `client/shared/styles/*` is available for imports from `imports.js`

If `imports.js` imports CSS, Tachyon emits `/imports.css` and links it from generated HTML shells.

Two shared-asset filenames get special treatment in generated shells:

- `client/shared/assets/favicon.svg` (or `app-icon.svg`) is linked as the
  favicon and Apple touch icon.
- `client/shared/assets/manifest.webmanifest` (or `manifest.json`) is linked
  as the web app manifest, and its `theme_color` becomes a
  `<meta name="theme-color">` tag. Combined with the built-in service worker,
  this makes the app an installable PWA.

The website showcase uses a local `imports.js` plus shared assets/data. Keep
demo-only browser helpers out of published runtime code; shared production
styles should live under `client/shared/assets` or `client/shared/styles`.

---

<details>
<summary><h2 style="display:inline">Build Output</h2></summary>

`ty bundle` writes a static-ready `dist/` directory. The default bundle target
is `web`. To prepare the same Tac output for native shell packaging, pass
`--target`:

```bash
ty bundle --target MacOS
ty bundle --target windows
ty bundle --target linux
ty bundle --target android
ty bundle --target ios
ty bundle --target all
```

Targets are case-insensitive. Supported values are `web`, `macos`, `windows`,
`linux`, `android`, `ios`, and `all`. `all` expands to every supported target.
Each target is emitted as a self-contained app under `dist/<target>/`, so a
multi-target build creates directories such as `dist/web`, `dist/ios`,
`dist/android`, `dist/macos`, `dist/windows`, and `dist/linux`.
Wasm workers can read the selected target with `request.platform("target")` or
the full comma list with `request.platform("targets")`.

Tac page and component scripts can read the same bundle context through
`this.tac.platform`, and templates can use the string globals `environment`,
`platform`, `target`, and `os` without imports:

```html
<logic :if="environment === 'desktop'">
  <p>Running the {platform} desktop bundle.</p>
</logic>
<logic :else-if="environment === 'mobile'">
  <p>Running the {platform} mobile bundle.</p>
</logic>
<logic else>
  <p>Running the {platform} bundle in a browser environment.</p>
</logic>
```

```js
// client/pages/tac.js
export default class {
  get layoutClass() {
    return `shell-${this.tac.platform.environment}`
  }
}
```

The `platform` value is one of `web`, `macos`, `windows`, `linux`, `ios`, or
`android`. The `environment` value is `browser`, `desktop`, or `mobile`.
`macos`, `windows`, and `linux` use the `desktop` environment; `ios` and
`android` use the `mobile` environment; `web` uses the `browser` environment.
The web platform also detects the visitor OS at runtime when the browser
exposes enough user-agent information.

For native targets, `ty bundle` produces a native webview host at
`dist/<target>/`:

```text
dist/macos/      (macOS WKWebView host)
dist/windows/    (Windows WebView2 host)
dist/linux/      (Linux WebKitGTK host)
dist/ios/        (iOS WKWebView host)
dist/android/    (Android WebView host)
```

These hosts are frontend-only shells: they load the static Tac files (embedded
under `Resources/`) through the operating system's webview (`WKWebView`,
WebView2, WebKitGTK, iOS `WKWebView`, or Android `WebView`). They do not embed
or spawn a Yon backend. Pass `--skip-native-host` to emit the plain web bundle
at `dist/<target>/` instead, or run `ty native-bundle --target <target>` later
to turn an existing `dist/<target>/` web bundle into the native host in place.

### Native artifact export (`.apk` / `.ipa` / `.app` / Linux binary)

Production bundles (`NODE_ENV=production`, or any bundle run with `--package`)
build distributable artifacts for native targets when the toolchains exist on
the machine, and `dist/<target>/` then contains **only the artifact** — no
project scaffolding. Pass `--skip-package` to opt out. Missing toolchains
downgrade to a logged skip, in which case the generated host project is kept
and remains buildable by hand.

- **Android** — requires a JDK, Gradle (`brew install gradle@8`), and an
  Android SDK (`brew install --cask android-commandlinetools`, or Android
  Studio, with `ANDROID_HOME` set). Emits `dist/android/<App>-<version>.apk`.
  Release builds are signed with the debug keystore by default so they
  install out of the box; provide `TAC_ANDROID_KEYSTORE`,
  `TAC_ANDROID_KEYSTORE_PASSWORD`, `TAC_ANDROID_KEY_ALIAS`, and
  `TAC_ANDROID_KEY_PASSWORD` for store signing.
- **iOS** — requires full Xcode plus `xcodegen` (`brew install xcodegen`).
  The generated host ships a `project.yml`; the packager runs
  `xcodegen generate` and `xcodebuild` to emit
  `dist/ios/<App>-<version>-unsigned.ipa`. Set `TAC_IOS_TEAM_ID` to produce a
  signed build through your Apple Developer team instead.
- **macOS** — requires the Xcode Command Line Tools (`swiftc`). Emits an
  ad-hoc-signed `dist/macos/<App>.app`.
- **Linux** — requires CMake plus GTK/WebKitGTK development packages
  (`apt-get install cmake libgtk-3-dev libwebkit2gtk-4.1-dev`), building on
  Linux. Emits `dist/linux/<App>/` containing the executable and its
  `Resources/`.
- **Windows** — requires Visual Studio 2022 with the C++ workload, CMake,
  and the WebView2 SDK, building on Windows. Emits `dist/windows/<App>/`
  containing `<App>.exe` and its `Resources/`.

`ty serve --no-bundle` or `YON_SKIP_BUNDLE=true` starts the server without
regenerating `dist/`, which is useful when another build pipeline owns frontend
output. For post-processing that should run after every Tachyon bundle, export a
`postBundle` hook from `tac.config.js`:

```js
export default {
  async postBundle({ distRoot, targetRoots }) {
    // patch targetRoots.web/index.html, write runtime config, copy deployment assets, etc.
  }
}
```

Typical output:

```text
dist/
  web/
    index.html
    docs/index.html
    pages/tac.js
    pages/docs/tac.js
    components/clicker/tac.js
    shared/modules/*.js
    shared/assets/*
    shared/data/*
    workers/language/rust/tac.worker.js
    workers/language/rust/tac.wasm
    fylo-browser-worker.js
    spa-renderer.js
    imports.js
    imports.css
  ios/
    Resources/*
    Sources/*
    tachyon.host.json
  android/
    Resources/*
    app/*
    tachyon.host.json
```

> **Notes:**
>
> - there is no `dist/layouts/` output
> - page shells are embedded into `spa-renderer.js`
> - static assets are emitted under `dist/<target>/shared/assets/`
> - the runtime now uses one app shell template and injects the HMR client only in development

</details>

---

## Commands

<table>
<tr><th align="left">Command</th><th align="left">Description</th></tr>
<tr><td><code>ty serve</code></td><td>Detects <code>client/</code> and <code>server/</code> contents, serves frontend, backend, or full-stack app</td></tr>
<tr><td><code>ty bundle</code></td><td>Builds <code>dist/</code></td></tr>
<tr><td><code>ty bundle --target MacOS</code></td><td>Builds a macOS <code>WKWebView</code> host at <code>dist/macos</code></td></tr>
<tr><td><code>ty bundle --target linux</code></td><td>Builds a Linux WebKitGTK host at <code>dist/linux</code></td></tr>
<tr><td><code>ty bundle --target android</code></td><td>Builds an Android WebView host at <code>dist/android</code></td></tr>
<tr><td><code>ty bundle --target ios</code></td><td>Builds an iOS <code>WKWebView</code> host at <code>dist/ios</code></td></tr>
<tr><td><code>ty bundle --target all</code></td><td>Builds the web bundle and native webview hosts for web, macOS, Windows, Linux, Android, and iOS</td></tr>
<tr><td><code>ty bundle --target macos --skip-native-host</code></td><td>Builds the plain web bundle at <code>dist/macos</code> without generating the native host</td></tr>
<tr><td><code>ty native-bundle --target macos</code></td><td>Turns an existing <code>dist/macos</code> web bundle into the native host in place</td></tr>
<tr><td><code>ty bundle --watch</code></td><td>Keeps <code>dist/</code> fresh</td></tr>
<tr><td><code>ty preview</code></td><td>Serves <code>dist/web</code></td></tr>
<tr><td><code>ty preview --target macos</code></td><td>Serves the selected target's previewable web assets, usually <code>dist/&lt;target&gt;/Resources</code> for native hosts</td></tr>
<tr><td><code>ty preview --watch --target web</code></td><td>Rebuilds and previews the selected target together</td></tr>
</table>

`ty preview --target <native>` checks local prerequisites before starting. For
example, macOS preview requires macOS plus `swiftc`, iOS requires Xcode,
Linux requires GTK/WebKitGTK development packages, Windows requires WebView2,
and Android requires an Android SDK plus JDK. Use `--skip-native-checks` only
when you intentionally want to serve the generated web assets without verifying
the native host toolchain.

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
- JavaScript and polyglot process middleware
- middleware-provided distributed rate limiting
- cache headers for runtime assets, chunks, shared assets, and shared data
- document-request detection using browser navigation headers such as `Sec-Fetch-Dest` / `Sec-Fetch-Mode`, with `Accept: text/html` kept as a fallback

---

## Middleware

Existing JavaScript middleware still works from `middleware.js`:

```js
export default {
  before(request, context) {
    if (request.headers.get('x-blocked') === 'true') {
      return Response.json({ detail: 'blocked' }, { status: 403 })
    }
  },
  after(request, response, context) {
    const headers = new Headers(response.headers)
    headers.set('X-Request-Id', context.requestId)
    return new Response(response.body, { status: response.status, headers })
  },
  rateLimiter: {
    take(request, context) {
      return null
    },
  },
}
```

Polyglot middleware can live at `server/middleware/yon.<ext>` or
`middleware/yon.<ext>`. JavaScript, TypeScript, Python, Ruby, and PHP can use a
class-style adapter:

```python
class Middleware:
    @staticmethod
    def before(input):
        if input["request"]["headers"].get("x-blocked") == "true":
            return {
                "action": "respond",
                "status": 403,
                "body": { "detail": "blocked" },
            }

    @staticmethod
    def after(input):
        return { "headers": { "X-Request-Id": input["context"]["requestId"] } }

    @staticmethod
    def rateLimit(input):
        return {
            "allowed": True,
            "limit": 100,
            "remaining": 99,
            "resetAt": 1780000000000,
        }
```

Any other language can use the raw shim protocol: read one JSON envelope from
stdin and write one JSON result to stdout. The envelope includes `phase`,
`request`, `response` for `after`, and `context`.

```json
{ "phase": "before", "request": {}, "context": {} }
```

Supported results:

```json
{ "action": "continue" }
{ "action": "respond", "status": 403, "headers": {}, "body": {} }
{ "action": "replace", "response": { "status": 200, "headers": {}, "body": {} } }
{ "allowed": true, "limit": 100, "remaining": 99, "resetAt": 1780000000000 }
```

To preserve streaming responses and avoid accidentally consuming request
bodies before route handlers run, process middleware receives request/response
metadata by default, not body streams.

---

## Distributed Rate Limiting

Export a `rateLimiter` from `middleware.js`, or `rateLimit()` from polyglot
middleware, to use a shared backend.

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
- validate the built frontend with `ty preview` before deploy

---

## License

MIT
