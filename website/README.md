# Tachyon Website

Living Tachyon showcase and release-acceptance project for the Rust `ty`
binary. This app is
**frontend-only**: there is no `server/` and no `db/`. Tac companions run beside
their templates, and the browser provides local
data through document collections mirrored into the origin-private file system
(OPFS).

The interface is built with DUVAY Light-DOM web components. Its versioned CSS,
component bundle, behaviors script, and license are vendored under
`client/shared/assets/duvay/` so every target remains self-contained and works
without a CDN inside a `default-src 'self'` content-security policy.

## Structure

- `/` — marketing landing page: hero, feature cards, Yon backend runtime
  overview, device target matrix, and quickstart code tabs.
- `/atlas` — a redirect into the guided capability atlas. The indexable
  sections live at `/atlas/overview`, `/atlas/compose`, `/atlas/react`,
  `/atlas/connect`, `/atlas/store`, `/atlas/observe`, and `/atlas/extend`.
  Together they cover native inputs, a reactive canvas studio, Wikimedia SSE
  streaming, tab-to-tab realtime with OPFS-replayed history, portable polyglot
  companions, fetch cache policies, and client telemetry.
- `/docs` — a wrapper page (`<slot />`) hosting dynamic `_topic` routes fed
  from `client/shared/data/docs.json`.

The shell is polymorphic: desktop gets full app-bar navigation, while
below-desktop layouts use a right-side dropdown menu with roomier touch
targets. Content-level layouts use container queries, and the platform-aware
Tac globals stamp `data-environment` / `data-platform` onto `<body>`.

The same `client/` source bundles into `dist/web/`, `dist/macos/`,
`dist/windows/`, `dist/linux/`, `dist/ios/`, and `dist/android/`. The
server and handler behavior is covered independently by the Rust integration
fixtures and the neutral compatibility corpus.

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
```

## Testing

The repository's CI runs the website smoke, DOM, and SEO suites through the
same public binary workflow:

```sh
bun run test
```

The helper invokes `ty bundle .`, asserts `dist/web/`, and then verifies all 11
static and dynamic routes, runtime assets, mobile shell behavior, and generated
SEO files.

## Deployment

Build the website on a qualified host with every compiler reported ready by
`ty doctor website`. Deploy a prebuilt archive whose root is the contents of
`dist/web/` (for example, a manual AWS Amplify zip deployment). This repository
does not use a connected Amplify source-build recipe: a stock Amplify image
does not provide the pinned Rust, Dart, Kotlin/Wasm, Swift/Wasm, and .NET/Wasm
toolchains required by the real-language showcase.

## PWA

The site is installable: `client/shared/assets/manifest.webmanifest` is
auto-linked into every shell (with its `theme_color` meta), icons live beside
it (`icon.svg`, `icon-192.png`, `icon-512.png`, `favicon.svg`), and Tachyon's
built-in service worker caches assets for offline use on non-loopback hosts.
Being frontend-only, the installed app keeps working offline — companions and
the OPFS database need no network.

## Notes

- Page roots hold no reactive state: the Tac client owns rendering and live
  state belongs to explicitly mounted component companions. Bootstrap shells
  contain metadata and a render plan, with a clear `<noscript>` fallback.
- Everything the site stores — theme, drafts, visit counters, inventory
  documents, chat history, telemetry spans — lives in the visitor's browser
  (localStorage, sessionStorage, and IndexedDB).
- Temporary bundle locks and generated target artifacts are ignored.
