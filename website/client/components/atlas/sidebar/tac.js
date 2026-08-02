// @ts-check

export default class {
  /** @param {HTMLElement} root @param {AbortSignal} signal */
  hydrate(root, signal) {
    const update = () => {
      const current = location.pathname === '/atlas' ? '/atlas/overview' : location.pathname
      for (const link of root.querySelectorAll('.atlas-sidebar-list a')) {
        if (!(link instanceof HTMLAnchorElement)) continue
        const active = new URL(link.href, location.href).pathname === current
        link.classList.toggle('active', active)
        if (active) link.setAttribute('aria-current', 'page')
        else link.removeAttribute('aria-current')
      }
    }

    update()
    window.addEventListener('tachyon:navigate', update)
    window.addEventListener('popstate', update)
    signal.addEventListener('abort', () => {
      window.removeEventListener('tachyon:navigate', update)
      window.removeEventListener('popstate', update)
    }, { once: true })
  }
}
