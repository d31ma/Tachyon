export default class {
  columns() {
    return [
      {
        title: 'Learn',
        links: [
          { label: 'Introduction', href: '/docs/introduction' },
          { label: 'Routing', href: '/docs/routing' },
          { label: 'Tac views', href: '/docs/templates' },
          { label: 'Yon handlers', href: '/docs/yon' },
        ],
      },
      {
        title: 'Explore',
        links: [
          { label: 'Every feature', href: '/docs/features' },
          { label: 'Languages', href: '/docs/polyglot' },
          { label: 'Build targets', href: '/docs/devices' },
        ],
      },
      {
        title: 'Project',
        links: [
          { label: 'Source on GitHub', href: 'https://github.com/d31ma/Tachyon' },
          { label: 'FYLO', href: 'https://fylo.del.ma' },
          { label: 'SESAME', href: 'https://sesame.del.ma' },
          { label: 'HEIMDALL', href: 'https://heimdall.del.ma' },
        ],
      },
    ]
  }
}
