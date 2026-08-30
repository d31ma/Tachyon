// @ts-nocheck -- validated by the Tachyon compiler and website contract gates.
import languages from '/shared/data/languages.json' with { type: 'json' }

export default class {
  /** Which framework layer is on show. Persisted for the tab. */
  $layer = 'tac'
  /** Which language within that layer. */
  $selected = 'javascript'

  copyLabel = 'Copy'

  get layer() { return this.$layer }
  get selected() { return this.$selected }

  /**
   * The runtime calls a handler as `handler(event, ...declaredArguments)`, so
   * the dispatched event arrives before anything the template passes.
   */
  pickLayer(_event, value) {
    this.$layer = value
    // Every layer has a JavaScript entry, so this is always a valid landing point.
    this.$selected = 'javascript'
  }

  pick(_event, value) {
    this.$selected = value
  }

  async copy() {
    try {
      await navigator.clipboard.writeText(this.current().code)
      this.copyLabel = 'Copied'
    } catch {
      // A denied clipboard is the reader's choice rather than a fault, so the
      // button says what to press instead of failing silently.
      this.copyLabel = 'Press \u2318C'
    }
    await globalThis.__tachyonTac?.render()
    setTimeout(() => {
      this.copyLabel = 'Copy'
      void globalThis.__tachyonTac?.render()
    }, 2000)
  }

  /** The Tac layer's whole example: view, stylesheet and every companion. */
  suite() {
    return languages.tac.files
  }

  /** The Yon layer's handlers, one per language. */
  entries() {
    return languages.yon.entries ?? []
  }

  /**
   * Every Yon suite's files, flattened into one list.
   *
   * The card groups them by language itself and shows one language at a time,
   * which is why this is passed whole rather than per selection: a component
   * property is assigned once, and reassigning it on a tab click does nothing.
   */
  handlers() {
    return this.entries().flatMap((entry) => entry.files)
  }

  current() {
    const entries = this.entries()
    return entries.find((entry) => entry.id === this.$selected) ?? entries[0]
  }

  /** Every layer stays selectable when the chosen one has no entry. */

  /**
   * Lines in the longest sample of the current layer.
   *
   * Pinning the panel's height stops the page shifting as the reader clicks
   * through languages. Pinning it to the *longest* sample also means none of
   * them needs a scrollbar. Those were only ever in tension because the height
   * was guessed from the viewport instead of measured from the data.
   */
  longest() {
    const lines = this.entries().reduce(
      (most, entry) => Math.max(most, entry.code.split('\n').length),
      1,
    )
    // Returned as the declaration rather than the number: a view expression is
    // bounded, and building a string is not one of the things it can do.
    return '--lines: ' + lines
  }

  summary() {
    return this.$layer === 'tac'
      ? 'One route, on every target. The view and the stylesheet are the same everywhere; pick a platform to see the companion compiled for it.'
      : 'A route receives a request, a service owns a decision, a repository owns storage — declared with @Controller, @Service and @Repository, and checked by the compiler against where each file sits and what it reaches. Shown in the eight languages whose own syntax can carry an annotation.'
  }
}
