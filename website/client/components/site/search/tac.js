// @ts-nocheck -- validated by the Tachyon compiler and website contract gates.
import { searchDocs } from '/shared/scripts/navigation.js'

export default class {
  static __tachyonOnMount = ['shortcut']

  showing = false
  results = []
  term = ''
  searching = false
  timer = null

  /**
   * The runtime calls a handler as `handler(event, ...declaredArguments)`, so
   * the dispatched event arrives before anything the template passes.
   */
  open() {
    this.showing = true
    // The input only exists once the dialog has rendered.
    setTimeout(() => {
      if (this.showing) document.getElementById('site-search-input')?.focus()
    }, 0)
  }

  close() {
    clearTimeout(this.timer)
    this.showing = false
    this.results = []
    this.term = ''
    this.searching = false
    // Wait for the event's render to finish: the trigger is recreated when
    // the focused dialog is removed, so retaining the old button won't work.
    setTimeout(async () => {
      await globalThis.__tachyonTac?.render()
      if (!this.showing) document.querySelector('.site-search__trigger button')?.focus()
    }, 0)
  }

  dismiss(_event, key) {
    if (key === 'Escape') {
      this.close()
      void globalThis.__tachyonTac?.render()
    }
  }

  status() {
    if (this.searching) return 'Searching…'
    if (this.term.length > 0 && this.term.length < 2) return 'Keep typing…'
    if (this.term.length >= 2 && this.results.length === 0) return `Nothing matches “${this.term}”.`
    if (this.results.length > 0) return `${this.results.length} result${this.results.length === 1 ? '' : 's'}`
    return 'Search titles and body text across every guide.'
  }

  find(_event, value) {
    this.term = String(value ?? '')
    clearTimeout(this.timer)
    this.searching = this.term.length >= 2
    const query = this.term
    this.timer = setTimeout(async () => {
      if (query !== this.term) return
      this.results = searchDocs(query)
      this.searching = false
      await globalThis.__tachyonTac?.render()
    }, 120)
  }

  shortcut() {
    // The shortcut every documentation site has trained people to expect.
    addEventListener('keydown', (event) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        if (this.showing) this.close()
        else this.open()
        void globalThis.__tachyonTac?.render()
      }
    })
  }
}
