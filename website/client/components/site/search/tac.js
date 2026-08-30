export default class {
  static __tachyonOnMount = ['shortcut']

  showing = false
  results = []
  term = ''
  searching = false

  /**
   * The runtime calls a handler as `handler(event, ...declaredArguments)`, so
   * the dispatched event arrives before anything the template passes.
   */
  open() {
    this.showing = true
    // The input only exists once the dialog has rendered.
    setTimeout(() => document.getElementById('site-search-input')?.focus(), 0)
  }

  close() {
    this.showing = false
    this.results = []
    this.term = ''
  }

  dismiss(_event, key) {
    if (key === 'Escape') this.close()
  }

  status() {
    if (this.searching) return 'Searching…'
    if (this.term.length > 0 && this.term.length < 2) return 'Keep typing…'
    if (this.term.length >= 2 && this.results.length === 0) return `Nothing matches “${this.term}”.`
    if (this.results.length > 0) return `${this.results.length} result${this.results.length === 1 ? '' : 's'}`
    return 'Search titles and body text across every guide.'
  }

  async find(_event, value) {
    this.term = String(value ?? '').trim()
    if (this.term.length < 2) {
      this.results = []
      await this.tac.render()
      return
    }
    this.searching = true
    await this.tac.render()
    try {
      // Handler Protocol v1 carries no query string, so the term is a path
      // segment rather than a parameter.
      const response = await this.tac.fetch(`/api/search/${encodeURIComponent(this.term)}`, {}, {
        cache: 'cache-first',
      })
      const payload = await response.json()
      this.results = Array.isArray(payload.results) ? payload.results : []
    } catch {
      // An unreachable server should read as no results, not a broken dialog.
      this.results = []
    } finally {
      this.searching = false
      await this.tac.render()
    }
  }

  shortcut() {
    // The shortcut every documentation site has trained people to expect.
    addEventListener('keydown', (event) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        if (this.showing) this.close()
        else this.open()
        void this.tac.render()
      }
    })
  }
}
