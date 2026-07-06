// @ts-check

export default class {
  features = [
    {
      title: 'File-system routing',
      tag: 'Tac + Yon',
      href: '/docs/routing',
      body: 'Pages under client/pages and handlers under server/routes become routes automatically. Dynamic segments use _slug directories — this docs section is one.',
    },
    {
      title: 'Polyglot backends',
      tag: 'Tac + Yon',
      href: '/atlas',
      body: 'Yon routes speak JavaScript, TypeScript, Python, Rust, C++ and more on the server — and this site\'s own backend is Tac Workers running Wasm in your browser.',
    },
    {
      title: 'Reactive companions',
      tag: 'Tac',
      href: '/docs/companions',
      body: 'Plain classes beside each template. Assigning a field rerenders the component subtree; $ and $$ prefixes persist state to sessionStorage and localStorage.',
    },
    {
      title: 'Tac Workers',
      tag: 'Wasm',
      href: '/docs/workers',
      body: 'Browser-local backends written in Rust, JavaScript or TypeScript, compiled to Wasm by Tachyon itself — no toolchain — and invoked with fetch("tac://…").',
    },
    {
      title: 'Local-first data',
      tag: 'FYLO',
      href: '/docs/storage',
      body: 'fetch() caches into IndexedDB, and the injected fylo client mirrors documents into OPFS with cache-first, network-first, reload and no-store policies.',
    },
    {
      title: 'Observable by default',
      tag: 'Ops',
      href: '/atlas',
      body: 'Generated OpenAPI 3.1 docs and OTel spans on the server — while this site times its own workers and stores the spans as FYLO documents in your browser.',
    },
  ]
}
