# Tachyon Website

Living Tachyon showcase for the in-repo `@d31ma/tachyon` package. This app is
**frontend-only**: there is no `server/` and no `db/`. Tac Workers compiled
in-house to WebAssembly mimic the backend, and the compiler-injected FYLO
browser client mimics the database with document collections mirrored into the
origin-private file system (OPFS).

The interface is built with DUVAY Light-DOM web components. Its versioned CSS,
component bundle, behaviors script, and license are vendored under
`client/shared/assets/duvay/` so every target remains self-contained and works
without a CDN inside a `default-src 'self'` content-security policy.

## Structure

- `/` — marketing landing page: hero, feature cards, Yon backend runtime
  overview, device target matrix, and quickstart code tabs.
- `/atlas` — the guided capability atlas, running entirely in the browser:
  native inputs, a reactive canvas studio, Wikimedia SSE streaming,
  tab-to-tab realtime over BroadcastChannel with OPFS-replayed history, a Rust
  worker answering every HTTP verb from tac.wasm, polyglot worker runs, FYLO
  cache policies and mutations, a document lifecycle demo, and client-side
  telemetry spans stored as FYLO documents.
- `/docs` — a wrapper page (`<slot />`) hosting dynamic `_topic` routes fed
  from `client/shared/data/docs.json`.

The shell is polymorphic: desktop gets full app-bar navigation, while
below-desktop layouts use a right-side dropdown menu with roomier touch
targets. Content-level layouts use container queries, and the platform-aware
Tac globals stamp `data-environment` / `data-platform` onto `<body>`.

The same `client/` source bundles into `dist/web/`, `dist/macos/`,
`dist/windows/`, `dist/linux/`, `dist/ios/`, and `dist/android/`. The
full-stack backend showcase (polyglot Yon routes, server FYLO, realtime
mailboxes, OpenTelemetry) lives in the framework repo at
`tests/fixtures/fullstack/`, where the integration suite exercises it.

## Commands

The public workflow assumes app authors installed Tachyon's standalone `ty`
binary. No package-manager command is required to build or preview a Tachyon app:

```sh
ty serve
ty bundle --target web
ty bundle --target macos
ty bundle --target windows
ty bundle --target linux
ty bundle --target ios
ty bundle --target android
ty bundle --target all
ty preview --target web
ty preview --target macos
ty preview --target android
```

Backend-only and full-stack apps use the same binary:

```sh
ty serve
YON_VALIDATE=true ty serve
YON_DATA_BROWSER_ENABLED=true ty serve
YON_REALTIME_ENABLED=true ty serve
YON_OTEL_ENABLED=true ty serve
```

## Testing

The repository's CI runs the website smoke, DOM and typechecking suites. App
authors consuming the binary should use `ty bundle` and `ty preview` for local
validation.

## PWA

The site is installable: `client/shared/assets/manifest.webmanifest` is
auto-linked into every shell (with its `theme_color` meta), icons live beside
it (`icon.svg`, `icon-192.png`, `icon-512.png`, `favicon.svg`), and Tachyon's
built-in service worker caches assets for offline use on non-loopback hosts.
Being frontend-only, the installed app keeps working offline — workers and
the OPFS database need no network.

## Notes

- `@d31ma/tachyon` is installed as `link:..` so the website always uses the
  framework code being developed in this repository.
- Page roots hold no reactive state: live state lives in components, the Tac
  re-render boundary, so the shared shell renders once per navigation.
- Everything the site stores — theme, drafts, visit counters, inventory
  documents, chat history, telemetry spans — lives in the visitor's browser
  (localStorage, sessionStorage, IndexedDB, and OPFS through FYLO).
- Temporary bundle locks and generated target artifacts are ignored.
