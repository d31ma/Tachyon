// @ts-check

export default class {
  /** @type {Map<string, string>} */
  searchIndex = new Map()

  /** @param {HTMLElement} root @param {AbortSignal} signal */
  async hydrate(root, signal) {
    const input = root.querySelector('input[type="search"]')
    const updateActive = () => {
      const current = location.pathname
      for (const link of root.querySelectorAll('.docs-sidebar-list a')) {
        const active = new URL(link.href, location.href).pathname === current
        link.classList.toggle('active', active)
        if (active) link.setAttribute('aria-current', 'page')
        else link.removeAttribute('aria-current')
      }
    }
    const filter = () => {
      const query = input instanceof HTMLInputElement ? input.value.trim().toLowerCase() : ''
      let visible = 0
      for (const item of root.querySelectorAll('[data-topic]')) {
        const slug = item.getAttribute('data-topic') ?? ''
        const searchable = this.searchIndex.get(slug) ?? item.textContent?.toLowerCase() ?? ''
        item.hidden = Boolean(query) && !searchable.includes(query)
        if (!item.hidden) visible += 1
      }
      for (const section of root.querySelectorAll('.docs-sidebar-section')) {
        section.hidden = !section.querySelector('[data-topic]:not([hidden])')
      }
      const empty = root.querySelector('.docs-no-results')
      if (empty instanceof HTMLElement) empty.hidden = visible !== 0
    }

    input?.addEventListener('input', filter, { signal })
    window.addEventListener('tachyon:navigate', updateActive, { signal })
    window.addEventListener('popstate', updateActive, { signal })
    updateActive()

    try {
      const response = await fetch('/shared/data/docs.json', { cache: 'reload', signal })
      const payload = await response.json()
      for (const [slug, topic] of Object.entries(payload.topics ?? {})) {
        this.searchIndex.set(slug, JSON.stringify(topic).toLowerCase())
      }
      filter()
    } catch (error) {
      if (!signal.aborted) console.warn('Unable to load the documentation search index.', error)
    }
  }
}
