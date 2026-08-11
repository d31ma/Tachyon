// @ts-check

const samples = {
  template: `<section class="hero">
  <h1>{headline}</h1>
  <button on:click="refresh()">Refresh</button>
</section>

<loop :for="post of posts">
  <product-card :product="post" hydrate="visible" />
</loop>`,
  companion: `export default class {
  headline = "Tac + Yon"
  visits = 0

  constructor(_props, tac) {
    tac.onMount(() => {
      this.visits = Number(sessionStorage.getItem("visits") ?? 0) + 1
      sessionStorage.setItem("visits", String(this.visits))
    })
  }
}`,
  yon: `// server/routes/posts/yon.js  ->  /posts
export class Handler {
  static GET() {
    return { posts: [] }
  }

  static async POST(request) {
    return { created: request.body }
  }
}`,
  polyglot: `// client/components/counter/tachyon-wasm.swift
var count: Int = 0

func increment() {
  count += 1
}

let tac: [String: TacMember] = [
  "count": .field({ count }, { count = $0 }),
  "increment": .method(increment),
]`,
}

function populateSamples() {
  for (const code of document.querySelectorAll('[data-sample]')) {
    const name = /** @type {keyof typeof samples | undefined} */ (code.getAttribute('data-sample') ?? undefined)
    if (name && samples[name]) code.textContent = samples[name]
  }
}

export default class {
  /** @param {Record<string, unknown>} _props @param {{ onMount(callback: () => void): void }} tac */
  constructor(_props, tac) {
    tac.onMount(populateSamples)
  }
}
