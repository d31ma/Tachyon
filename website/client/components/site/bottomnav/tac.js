// @ts-check

export default class {
  /** @type {string} */
  active = 'home'

  constructor() {
    if (typeof location !== 'undefined') {
      this.active = this.sectionFrom(location.pathname)
    }
  }

  /** @param {string} pathname @returns {string} */
  sectionFrom(pathname) {
    if (pathname.startsWith('/atlas')) return 'atlas'
    if (pathname.startsWith('/docs')) return 'docs'
    return 'home'
  }

  @onMount
  trackNavigation() {
    window.addEventListener('tachyon:navigate', (event) => {
      const detail = /** @type {CustomEvent<{ pathname?: string }>} */ (event).detail
      this.active = this.sectionFrom(detail?.pathname ?? location.pathname)
    })
  }
}
