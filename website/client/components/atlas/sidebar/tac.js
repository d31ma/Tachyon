// @ts-check

// The shell re-renders once per navigation (docs pattern), so reading the
// active slug in the constructor keeps the highlight in sync without state.
export default class {
  sections = [
    { slug: 'overview', index: '00', label: 'Overview', summary: 'What the atlas is and live stats' },
    { slug: 'compose', index: '01', label: 'Compose', summary: 'Native semantics with reactive Tac state' },
    { slug: 'react', index: '02', label: 'React', summary: 'Rerenders, persistence and live streams' },
    { slug: 'connect', index: '03', label: 'Connect', summary: 'Polyglot companions on one controller ABI' },
    { slug: 'store', index: '04', label: 'Store', summary: 'FYLO collections mirrored into OPFS' },
    { slug: 'observe', index: '05', label: 'Observe', summary: 'Client-side telemetry spans in FYLO' },
    { slug: 'extend', index: '06', label: 'Extend', summary: 'Scoped and lazy Tac companions' },
  ]

  /** @type {string} */
  activeSlug = ''

  constructor() {
    if (typeof location !== 'undefined') {
      this.activeSlug = this.slugFrom(location.pathname)
    }
  }

  /** @param {string} pathname @returns {string} */
  slugFrom(pathname) {
    const [, , slug] = pathname.split('/')
    return slug || 'overview'
  }

  @onMount
  trackNavigation() {
    // SPA navigations swap the slot content but keep this wrapper-page
    // component alive, so follow the runtime's navigation events to keep
    // the active section highlighted (docs sidebar pattern).
    window.addEventListener('tachyon:navigate', (event) => {
      const detail = /** @type {CustomEvent<{ pathname?: string }>} */ (event).detail
      this.activeSlug = this.slugFrom(detail?.pathname ?? location.pathname)
    })
  }
}
