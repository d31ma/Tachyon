# Tachyon

Tachyon is a **polyglot, file-system-routed full-stack framework for [Bun](https://bun.sh)**. It lets you define API routes as plain executable files written in any language, and build reactive front-end pages with a lightweight HTML template syntax — all without configuration.

## Features

- **File-system routing** — routes are directories; HTTP methods are files
- **Polyglot handlers** — write routes in Bun, Python, Ruby, Go, Rust, Java, or any language with a shebang
- **Reactive front-end (Yon)** — HTML templates with bindings, loops, conditionals, and custom components
- **Lazy component loading** — defer component rendering until visible with `IntersectionObserver`
- **NPM dependency bundling** — use npm packages in front-end code via `/modules/` imports
- **Hot Module Replacement** — watches `routes/` and `components/` and reloads on change
- **Custom 404 page** — drop a `404.html` in your project root to override the default
- **Schema validation** — per-route request/response validation via `OPTIONS` files
- **Auth** — built-in Basic Auth and JWT decoding
- **Streaming** — SSE responses via `Accept: text/event-stream`

## Installation

```bash
npm install @vyckr/tachyon
```

## Quick Start

```bash
# Start the development server (expects routes/ in the current directory)
tach.serve

# Build front-end assets into dist/
tach.bundle
```

Or via npm scripts if you declare them in your own `package.json`:

```json
{
  "scripts": {
    "start":  "tach.serve",
    "bundle": "tach.bundle"
  }
}
```

## Configuration

Create a `.env` file in your project root. All variables are optional.

```env
PORT=8000
HOSTNAME=127.0.0.1
TIMEOUT=70
DEV=true

# CORS
ALLOW_HEADERS=*
ALLOW_ORIGINS=*
ALLOW_CREDENTIALS=false
ALLOW_EXPOSE_HEADERS=*
ALLOW_MAX_AGE=3600
ALLOW_METHODS=GET,POST,PUT,DELETE,PATCH,OPTIONS

# Auth
BASIC_AUTH=username:password

# Validation (set to any value to enable)
VALIDATE=true

# Custom route/asset paths (defaults to <cwd>/routes, <cwd>/components, <cwd>/assets)
ROUTES_PATH=
COMPONENTS_PATH=
ASSETS_PATH=
```

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
    "ipAddress": "127.0.0.1",
    "bearer":    { "header": {}, "payload": {}, "signature": "..." }
  }
}
```

### Route Handler Examples

**Bun (TypeScript)**
```typescript
// routes/v1/:collection/GET
#!/usr/bin/env bun

const { body, paths, context } = await Bun.stdin.json()

const response = { collection: paths.collection, from: context.ipAddress }

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

### Template Syntax

| Syntax | Description |
|--------|-------------|
| `{expr}` | Interpolate expression |
| `@event="handler()"` | Event binding |
| `:prop="value"` | Bind attribute to expression |
| `:value="variable"` | Two-way input binding |
| `<loop :for="...">` | Loop block |
| `<logic :if="...">` | Conditional block |
| `<myComp_ prop=val />` | Custom component (trailing `_`) |
| `<myComp_ lazy />` | Lazy-loaded component (renders when visible) |

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
<counter_ />
```

### Lazy Loading

Add the `lazy` attribute to defer a component's loading until it scrolls into view. The component renders a lightweight placeholder and uses `IntersectionObserver` to load the module on demand.

```html
<!-- Eager (default) — loaded immediately -->
<counter_ />

<!-- Lazy — loaded when visible in the viewport -->
<counter_ lazy />
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

Outputs compiled assets to `dist/`.

## License

MIT
