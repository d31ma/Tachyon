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
      body: 'Yon keeps backend routes file-routed and language-flexible, while this site dogfoods a frontend-only shape with Tac companions running directly beside their templates.',
    },
    {
      title: 'Reactive companions',
      tag: 'Tac',
      href: '/docs/companions',
      body: 'Plain classes beside each template. Assigning a field rerenders the component subtree; $ and $$ prefixes persist state to sessionStorage and localStorage.',
    },
    {
      title: 'Polyglot companions',
      tag: 'Tac',
      href: '/docs/polyglot',
      body: 'Use JavaScript, TypeScript, Dart, Rust, Kotlin, Swift or C# beside a Tac template. Every supported companion receives the same reactive controller and device API.',
    },
    {
      title: 'Native HTML rendering',
      tag: 'Tac Native',
      href: '/docs/native-rendering',
      body: 'Keep authoring strict HTML. Non-web bundles lower supported nodes into SwiftUI, Jetpack Compose, WinUI or GTK controls and isolate unsupported subtrees in local WebView boundaries.',
    },
    {
      title: 'Island hydration',
      tag: 'Tac Web',
      href: '/docs/templates',
      body: 'Server-render every component, then activate it on load, idle, visibility or interaction — or keep it permanently static — with one literal hydrate policy.',
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
      body: 'Generated OpenAPI 3.1 docs and OTel spans on the server — while this site times companion fetches and stores the spans as FYLO documents in your browser.',
    },
  ]
}
