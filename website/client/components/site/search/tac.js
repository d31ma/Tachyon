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
    setTimeout(() => this.bindInput(), 0)
  }

  close() {
    clearTimeout(this.timer)
    this.showing = false
    this.results = []
    this.term = ''
    this.searching = false
  }

  dismiss(_event, key) {
    if (key === 'Escape') {
      this.close()
      void globalThis.__tachyonTac?.render()
    }
  }

  /**
   * Keep keystrokes outside Tachyon's auto-rendering event bridge. Replacing
   * the input after each native `input` event would drop focus mid-query.
   */
  bindInput() {
    const input = document.getElementById('site-search-input')
    if (!input) return
    input.focus()
    input.setSelectionRange?.(this.term.length, this.term.length)
    if (input.dataset.searchBound) return
    input.dataset.searchBound = 'true'
    input.addEventListener('input', (event) => this.find(event, event.target?.value))
    input.addEventListener('keydown', (event) => this.dismiss(event, event.key))
  }

  status() {
    if (this.searching) return 'Searching…'
    if (this.term.length > 0 && this.term.length < 2) return 'Keep typing…'
    if (this.term.length >= 2 && this.results.length === 0) return `Nothing matches “${this.term}”.`
    if (this.results.length > 0) return `${this.results.length} result${this.results.length === 1 ? '' : 's'}`
    return 'Search titles and body text across every guide.'
  }

  find(_event, value) {
    this.term = String(value ?? '').trim()
    clearTimeout(this.timer)
    this.searching = this.term.length >= 2
    const query = this.term
    this.timer = setTimeout(async () => {
      if (query !== this.term) return
      this.results = searchDocs(query)
      this.searching = false
      await globalThis.__tachyonTac?.render()
      setTimeout(() => this.bindInput(), 0)
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
