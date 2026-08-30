# Tachyon website

The framework's own site, built with Tachyon: Tac views in the browser, Yon
handlers on the server, and a Fylo document store behind both.

## Shape

```text
client/
  pages/            routes: /, /docs, /docs/:topic, /docs/features/:id
  components/       site shell, home sections, docs sidebar/topic/snippet
  shared/           styles, vendored DuVay, authored docs and language data
server/
  routes/api/       Yon handlers: search, versions, uptime, checksum
  vendor/fylo.mjs   vendored Fylo Node shim
scripts/seed.mjs    seeds the Fylo store
tac.config.js       application identity, and the postBundle discovery hook
```

## Running it

```sh
bun install
bun run seed     # writes the Fylo store, once
bun run dev
```

`bun run styles` regenerates the Tailwind stylesheet after changing a template;
`bun run styles:check` fails when the committed file is stale.

## The data layer

Search is served by a Yon handler reading a Fylo store. The store lives at
`/Volumes/DATA/tachyon-site` by default and moves with `TAC_SITE_STORE`.

The guides stay authored as data in `client/shared/data/docs.json` — they belong
with the code that documents them. What the store holds is the *search index*
derived from them.

Two contract details a handler here has to respect:

- **Handler Protocol v1 carries no query string.** A request has a route,
  method, headers, dynamic `parameters` and an optional body. Search therefore
  takes its term as a path segment — `/api/search/:query` — rather than `?q=`.
- **A handler is loaded as a `data:` URL**, where a relative import has no base
  to resolve against. The Fylo shim is imported by absolute path, built from the
  working directory, which is the project root.

## Snippets are compiled, not written

`bun run verify:snippets` puts every published example through the real
toolchain: Tac templates are built as routes, and handler snippets are parsed by
their own runtime. A documented API that does not compile is worse than no
documentation, so this runs before the site ships.

## Styling

DuVay for rigidity, Tailwind for fluidity, ordered explicitly because component
classes and utilities share specificity:

```css
@layer duvay, theme, base, components, brand, utilities;
```

DuVay's design tokens are exposed as Tailwind utilities in `tailwind.src.css`,
so a template reaches for `text-sm` or `border-line` instead of an inline style,
and both follow the light/dark switch. Tailwind is imported without Preflight:
DuVay owns the base layer.

## Discovery

`postBundle` in `tac.config.js` writes `sitemap.xml` and `robots.txt` from the
route manifest and the guide list, so they describe the site that was actually
built. Every page carries a canonical URL and Open Graph and Twitter metadata.
