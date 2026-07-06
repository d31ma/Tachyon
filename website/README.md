# TAC-H-YON Website

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

- `/` — marketing landing page: hero, feature cards, quickstart code tabs.
- `/atlas` — the guided capability atlas, running entirely in the browser:
  native inputs, a reactive canvas studio, Wikimedia SSE streaming,
  tab-to-tab realtime over BroadcastChannel with OPFS-replayed history, a Rust
  worker answering every HTTP verb from tac.wasm, polyglot worker runs, FYLO
  cache policies and mutations, a document lifecycle demo, and client-side
  telemetry spans stored as FYLO documents.
- `/docs` — a wrapper page (`<slot />`) hosting dynamic `_topic` routes fed
  from `client/shared/data/docs.json`.

The shell is polymorphic: desktop gets full app-bar navigation, tablet gets an
off-canvas drawer, and phones get bottom navigation with roomier touch
targets. Content-level layouts use container queries, and the platform-aware
Tac globals stamp `data-environment` / `data-platform` onto `<body>`.

The same `client/` source bundles into `dist/web/`, `dist/macos/`,
`dist/windows/`, `dist/linux/`, `dist/ios/`, and `dist/android/`. The
full-stack backend showcase (polyglot Yon routes, server FYLO, realtime
mailboxes, OpenTelemetry) lives in the framework repo at
`tests/fixtures/fullstack/`, where the integration suite exercises it.

## Scripts

- `bun run serve`
  Runs the Tachyon dev server (shape-aware: frontend-only).
- `bun run start`
  Alias for `bun run serve`.
- `bun run bundle`
  Builds `dist/web/` by default. Pass `-- --target all` for every platform.
- `bun run preview`
  Rebuilds and previews the selected target; web is the default.

## Testing

- `bun test`
  Runs the Bun smoke suite and the bundled DOM regression suite.
- `bun run test:dom`
  Runs only the DOM regression suite.
- `bunx tsc --noEmit`
  Typechecks client and tests with strict JSDoc.

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
