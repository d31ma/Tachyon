// @ts-check

export default class {
  /** @type {string} */
  $active = 'template'

  get active() {
    return this.$active
  }

  tabs = [
    { id: 'template', label: 'Tac template' },
    { id: 'companion', label: 'Companion' },
    { id: 'route', label: 'Yon route' },
    { id: 'polyglot', label: 'Polyglot companion' },
  ]

  samples = {
    template: [
      '<section class="hero">',
      '  <h1>{headline}</h1>',
      '  <button on:click="refresh()">Refresh</button>',
      '</section>',
      '',
      '<loop :for="post of posts">',
      '  <article>{post.title}</article>',
      '</loop>',
    ].join('\n'),
    companion: [
      'export default class {',
      '  headline = "Tac + Yon"',
      '  $visits = 0        // sessionStorage',
      '  $$theme = "light"  // localStorage',
      '',
      '  async refresh() {',
      '    const res = await this.fetch("/posts")',
      '    this.posts = (await res.json()).posts',
      '  }',
      '}',
    ].join('\n'),
    route: [
      '// server/routes/posts/yon.js  ->  /posts',
      'export class Handler {',
      '  static GET() {',
      '    return { posts: [] }',
      '  }',
      '',
      '  static async POST(request) {',
      '    return { created: request.body }',
      '  }',
      '}',
    ].join('\n'),
    polyglot: [
      '// client/components/counter/tac.swift',
      'final class Counter: Tac {',
      '  var count: Int = 0',
      '',
      '  func increment() {',
      '    self.count += 1',
      '  }',
      '}',
    ].join('\n'),
  }

  /** @param {string} id */
  select(id) {
    this.$active = id
  }
}
