# Tachyon

Tachyon is a **polyglot, file-system-routed full-stack framework for [Bun](https://bun.sh)**. It lets you define API routes as plain executable files written in any language, and build reactive front-end pages with a lightweight HTML template syntax — all without configuration.

## Features

- **File-system routing** — routes are directories; HTTP methods are files
- **Polyglot handlers** — write routes in Bun, Python, Ruby, Go, Rust, Java, or any language with a shebang
- **Reactive front-end (Yon)** — HTML templates with bindings, loops, conditionals, and custom components
- **Lazy component loading** — defer component rendering until visible with `IntersectionObserver`
- **NPM dependency bundling** — use npm packages in front-end code via `/modules/` imports
- **Static HTML export** — `tach.bundle` prerenders each `HTML` route into `dist/**/index.html` for static hosting
- **Hot Module Replacement** — watches `routes/` and `components/` and reloads on change
- **Custom 404 page** — drop a `404.html` in your project root to override the default
- **Schema validation** — per-route request/response validation via `OPTIONS` files
- **Status code routing** — map response schemas to HTTP status codes; the framework picks the code automatically
- **Auth** — built-in Basic Auth (timing-safe) and JWT decoding with expiry enforcement
- **Security headers** — X-Frame-Options, X-Content-Type-Options, HSTS, CSP, and Referrer-Policy sent on every response
- **Streaming** — SSE responses via `Accept: text/event-stream`

## Installation

```bash
bun add @delma/tachyon
```

## Quick Start

```bash
# Scaffold a new app
tach.init my-app

# In a Tachyon app:
bun run bundle
bun run preview
bun run serve
bun run serve --full
```

Or via npm scripts if you declare them in your own `package.json`:

```json
{
  "scripts": {
    "bundle": "tach.bundle",
    "preview": "tach.preview --watch",
    "serve": "tach.serve"
  }
}
```

## Scaffolding a New App

```bash
bunx @delma/tachyon tach.init my-app
cd my-app
bun install
bun run serve
```

`tach.init` creates a starter project with:

- `routes/HTML`, `routes/LAYOUT`, and `routes/GET`
- a sample `components/hero.html`
- `main.js`
- `.env.example`
- `amplify.yml`
- `package.json` scripts for `serve`, `bundle`, and `preview`

## Configuration

Create a `.env` file in your project root. All variables are optional.

```env
PORT=8000
HOSTNAME=127.0.0.1
TIMEOUT=70
DEV=true
LOG_LEVEL=info
LOG_FORMAT=pretty

# CORS — restrict to explicit origins in production; never combine * with credentials
ALLOW_HEADERS=Content-Type,Authorization
ALLOW_ORIGINS=https://yourdomain.com
ALLOW_CREDENTIALS=false
ALLOW_EXPOSE_HEADERS=
ALLOW_MAX_AGE=3600
ALLOW_METHODS=GET,POST,PUT,DELETE,PATCH,OPTIONS

# Auth — generate strong credentials; never commit real values
BASIC_AUTH=

# Validation (set to any value to enable)
VALIDATE=true

# Security
# Override the default CSP if your app loads scripts/styles from external origins
CONTENT_SECURITY_POLICY=default-src 'self'
# Maximum ms a handler process may run before it is killed (default: 30000)
HANDLER_TIMEOUT_MS=30000
# Maximum length of any single route or query parameter value (default: 1000)
MAX_PARAM_LENGTH=1000

# Custom route/asset paths (defaults to <cwd>/routes, <cwd>/components, <cwd>/assets)
ROUTES_PATH=
COMPONENTS_PATH=
ASSETS_PATH=
```

`LOG_LEVEL` supports `trace`, `debug`, `info`, `warn`, `error`, `fatal`, and `silent`. `LOG_FORMAT` supports `pretty` for local development and `json` for production log pipelines. `TACHYON_LOG_LEVEL` and `TACHYON_LOG_FORMAT` are also accepted if you want framework-specific overrides.

Handler subprocess logs include per-request resource usage after each handler exits: `requestId`, handler `pid`, exit code, CPU time in microseconds, peak RSS memory in bytes, filesystem read/write operation counts, and response/error byte counts.

## Route Structure

```
routes/
  GET               →  GET  /
  POST              →  POST /
  api/
    GET             →  GET  /api
    :version/
      GET           →  GET  /api/:version
      DELETE        →  DELETE /api/:version
  dashboard/
    HTML            →  front-end page at /dashboard
  OPTIONS           →  schema file (optional, enables validation)
```

### Requirements

- Every route handler is an **executable file** — include a shebang on the first line
- The last path segment must be an **uppercase HTTP method** (e.g. `GET`, `POST`, `DELETE`) or `HTML` for a front-end page
- Dynamic segments start with `:` (e.g. `:version`, `:id`)
- The first path segment must **not** be dynamic
- Adjacent dynamic segments are not allowed (e.g. `/:a/:b/GET` is invalid)
- Node modules must be imported dynamically with the `/modules/` prefix: `await import('/modules/dayjs.js')`
- Components live in `components/` and must have a `.html` extension

### Request Context

Every handler receives the full request context on `stdin` as a JSON object:

```json
{
  "headers": { "content-type": "application/json" },
  "body":    { "name": "Alice" },
  "query":   { "page": 1 },
  "paths":   { "version": "v2" },
  "context": {
    "requestId": "3f5b52f8-9c2e-4f8d-8bd3-6fd2b10c28d9",
    "ipAddress": "127.0.0.1",
    "bearer": {
      "token": "...",
      "verified": false
    }
  }
}
```

Tachyon reuses an incoming `X-Request-Id` header when present, generates one when it is missing, returns it on every response, and includes it in request logs.

> **Note:** `context.bearer` exposes only the raw bearer token and `verified: false`. Tachyon may decode the payload internally to reject expired JWTs, but unverified claims are not exposed to handlers. Use middleware plus a verifier such as [`jose`](https://github.com/panva/jose) when handlers need authenticated identity.

### Route Handler Examples

**Bun (TypeScript)**
```typescript
// routes/v1/:collection/GET
#!/usr/bin/env bun

const { body, paths, context } = await Bun.stdin.json()

const response = { collection: paths.collection, from: context.ipAddress, requestId: context.requestId }

Bun.stdout.write(JSON.stringify(response))
```

**Python**
```python
# routes/v1/:collection/POST
#!/usr/bin/env python3
import json, sys

stdin = json.loads(sys.stdin.read())
sys.stdout.write(json.dumps({ "message": "Hello from Python!" }))
```

**Ruby**
```ruby
# routes/v1/:collection/DELETE
#!/usr/bin/env ruby
require 'json'

stdin = JSON.parse(ARGF.read)
print JSON.generate({ message: "Hello from Ruby!" })
```

### Schema Validation

Place an `OPTIONS` file in any route directory to enable validation:

```json
{
  "POST": {
    "req": {
      "name":   "string",
      "age?":   0
    },
    "res": {
      "message": "string"
    },
    "err": {
      "detail": "string"
    }
  }
}
```

Nullable fields are suffixed with `?`. Set `VALIDATE=true` in your `.env` to enable.

### Status Code Routing

Instead of `res`/`err`, you can key response schemas by HTTP status code. Tachyon matches the handler's JSON output against each schema in ascending order — the first match determines the response status code.

```json
{
  "POST": {
    "req": { "name": "string" },
    "201": { "id": "string", "name": "string" },
    "400": { "detail": "string" },
    "503": { "detail": "string", "retryAfter": 0 }
  },
  "DELETE": {
    "204": {}
  }
}
```

Handlers write their normal JSON to stdout — no changes required. The framework determines the status code from whichever schema the output matches. If no numeric schemas are defined, the default behaviour applies (stdout → 200, stderr → 500).

When `VALIDATE=true` is set, the matched schema is also used for strict validation.

## Front-end Pages (Yon)

Create an `HTML` file inside any route directory to define a front-end page:

```html
<!-- routes/HTML -->
<script>
  document.title = "Home"
  let count = 0
</script>

<h1>Count: {count}</h1>
<button @click="count++">Increment</button>
```

When you run `tach.bundle`, Tachyon compiles these pages into browser modules and also prerenders static HTML files such as:

```text
dist/
  index.html
  dashboard/index.html
  pages/HTML.js
  pages/dashboard/HTML.js
```

That means the bundled output is directly usable on static hosts while still keeping the SPA runtime available for client-side navigation and interactivity.

To preview the generated `dist/` output locally, run:

```bash
tach.preview
```

To serve `dist/` and keep rebuilding it from frontend source changes in one command, run:

```bash
tach.preview --watch
```

`tach.preview` serves exact bundle assets such as `/main.js` and also resolves nested route files like `/docs` to `dist/docs/index.html`.

## Development Commands

In a scaffolded Tachyon app, the recommended commands are:

```bash
bun run bundle
bun run preview
bun run serve
bun run serve --full
```

- `bun run bundle` builds the app into `dist/`
- `bun run preview` serves `dist/` and rebuilds it when frontend files change
- `bun run serve` starts the Tachyon app server only
- `bun run serve --full` serves the frontend bundle and backend API routes from the same port

### Yon Output Format

Yon emits ESM page, layout, and component modules by default. For script-tag/CDN-style environments, set `YON_FORMAT=global` before running `tach.bundle`, `tach.preview`, or `tach.serve`; generated frontend modules will register with the browser global `window.Yon` instead of exporting ESM defaults.

```bash
YON_FORMAT=global bun run bundle
```

The global format keeps the same runtime behavior, but compiled modules are classic-script compatible and can be resolved through `window.Yon.load('/pages/HTML.js')`.

### Template Syntax

| Syntax | Description |
|--------|-------------|
| `{expr}` | Interpolate and HTML-escape expression |
| `{!expr}` | Render trusted raw HTML without escaping |
| `@event="handler()"` | Event binding; handlers receive `$event` |
| `:prop="value"` | Bind attribute to expression |
| `:value="variable"` | Two-way input binding |
| `<loop :for="...">` | Loop block |
| `<logic :if="...">` | Conditional block |
| `<my-comp prop=val />` | Custom component from `components/my-comp.html` |
| `<my-comp lazy />` | Lazy-loaded component (renders when visible) |

### Custom Components

```html
<!-- components/counter.html -->
<script>
  let count = 0
</script>

<button @click="count++">Clicked {count} times</button>
```

Use in a page:

```html
<counter />
```

Components can emit custom events to their parent wrapper with `emit(name, detail)`.
Parent handlers receive the browser `CustomEvent` as `$event`, so payloads are
available at `$event.detail`.

```html
<!-- components/item-picker.html -->
<script>
  let label = 'Default'

  function choose() {
    emit('selected', { label })
  }
</script>

<button @click="choose()">Choose {label}</button>
```

```html
<!-- routes/HTML -->
<script>
  let selected = 'none'
</script>

<item-picker label="Tachyon" @selected="selected = $event.detail.label" />
<p>Selected: {selected}</p>
```

### Lazy Loading

Add the `lazy` attribute to defer a component's loading until it scrolls into view. The component renders a lightweight placeholder and uses `IntersectionObserver` to load the module on demand.

```html
<!-- Eager (default) — loaded immediately -->
<counter />

<!-- Lazy — loaded when visible in the viewport -->
<counter lazy />
```

Lazy components are fully interactive once loaded — event delegation and state management work identically to eager components.

### NPM Modules in Front-end Code

Any package listed in your project's `dependencies` is automatically bundled and served at `/modules/<name>.js`. Import them dynamically in your `<script>` blocks:

```html
<script>
  const { default: dayjs } = await import('/modules/dayjs.js')
  let timestamp = dayjs().format('MMM D, YYYY h:mm A')
</script>

<p>Last updated: {timestamp}</p>
```

### Custom 404 Page

Place a `404.html` file in your project root to override the default 404 page. It uses the same Yon template syntax:

```html
<!-- 404.html -->
<script>
  document.title = "Not Found"
</script>

<h1>Oops!</h1>
<p>This page doesn't exist.</p>
<a href="/">Go home</a>
```

If no custom `404.html` is found, Tachyon serves a built-in styled 404 page.

## Building for Production

```bash
tach.bundle
```

Outputs compiled assets to `dist/`, including prerendered route files such as `dist/index.html` and `dist/docs/index.html`.

To preview the built output locally:

```bash
tach.preview
```

To serve `dist/` and keep it rebuilding from source changes in one command:

```bash
tach.preview --watch
```

If you want to serve `dist/` with Bun's HTML/static tooling during development, keep the bundle fresh with:

```bash
tach.bundle --watch
```

That watch mode rebuilds `dist/` when files change in:

- `routes/`
- `components/`
- `assets/`
- `main.js`
- `package.json`

This is the mode to pair with a static server that watches `dist/`.

If you are building a full-stack Tachyon app and want the app server plus the frontend preview together, use:

```bash
tach.serve --full
```

That runs the normal Tachyon dev server while also serving the bundled frontend from `dist/` on the same port. Browser-style `Accept: text/html` requests receive the frontend, while API-style requests still hit the route handlers.
If you only need the static frontend preview workflow, `tach.preview --watch` is the simpler option.

When `NODE_ENV=production` is set without `--full`, Tachyon uses the production HTML shell fallback and does not inject the development HMR client. Use `tach.serve --full` when the same production server should serve bundled frontend assets from `dist/`.

### Static Hosting

The bundled output is designed to work on static hosts:

- `dist/index.html` serves the root route
- nested pages are emitted as `dist/<route>/index.html`
- browser modules and assets are emitted alongside them in `dist/pages`, `dist/layouts`, `dist/components`, `dist/assets`, and `dist/modules`

That means you can deploy `dist/` directly to platforms like Amplify, Netlify, Cloudflare Pages, GitHub Pages, or any CDN/object-store static host.

### AWS Amplify

An example Amplify build file is included at [examples/amplify.yml](/Users/iyor/Library/CloudStorage/Dropbox/myProjects/TACHYON/examples/amplify.yml).

Typical project setup:

```yaml
version: 1
frontend:
  phases:
    preBuild:
      commands:
        - curl -fsSL https://bun.sh/install | bash
        - export PATH="$HOME/.bun/bin:$PATH"
        - bun install --frozen-lockfile
    build:
      commands:
        - export PATH="$HOME/.bun/bin:$PATH"
        - bunx @delma/tachyon tach.bundle
  artifacts:
    baseDirectory: dist
    files:
      - '**/*'
```

If your app depends on a local `main.js`, components, layouts, or nested `HTML` routes, `tach.bundle` will include them automatically.

### Recommended Deploy Flow

```bash
tach.bundle
tach.preview
```

Use `tach.preview` to verify:

- `/` resolves to the prerendered homepage
- nested routes like `/docs` resolve to `dist/docs/index.html`
- assets such as `/main.js` and `/assets/*` load correctly

Once that looks good, deploy the `dist/` directory.

## Security

Tachyon applies the following protections by default:

| Area | Protection |
|------|-----------|
| **Response headers** | `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, `Content-Security-Policy`, `Referrer-Policy` on every response; `Strict-Transport-Security` is opt-in with `ENABLE_HSTS=true` |
| **Basic Auth** | Credential comparison uses `timingSafeEqual` to prevent timing oracle attacks |
| **JWT** | Raw bearer tokens are exposed with `verified: false`; expired JWTs are rejected when their `exp` claim can be decoded |
| **Request body limits** | Request bodies exceeding `MAX_BODY_BYTES` return HTTP 413 before handler execution |
| **Template escaping** | Text interpolation and dynamic attributes are escaped by default; raw HTML requires `{!expr}` |
| **Process timeout** | Handler processes that exceed `HANDLER_TIMEOUT_MS` are killed automatically |
| **Parameter limits** | Query and path parameters exceeding `MAX_PARAM_LENGTH` characters return HTTP 400 |
| **Error responses** | Unhandled server errors and handler `stderr` failures return generic messages; internal details are logged server-side with the request id |
| **HMR** | Development HMR defaults to `127.0.0.1`, limits clients with `HMR_MAX_CLIENTS`, and requires `HMR_TOKEN` when exposed beyond loopback |
| **CORS** | Wildcard `ALLOW_ORIGINS=*` combined with `ALLOW_CREDENTIALS=true` is not recommended — set explicit origins in production |

For production deployments:
- Set `BASIC_AUTH` to a strong credential — never use a default value
- Set `ALLOW_ORIGINS` to your application's domain instead of `*`
- Set `ENABLE_HSTS=true` only when serving HTTPS directly or behind a trusted HTTPS proxy
- Consider adding a reverse proxy (nginx, Caddy) to enforce HTTPS and add rate limiting

## License

MIT
