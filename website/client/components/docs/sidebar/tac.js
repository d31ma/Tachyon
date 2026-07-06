// @ts-check

/** @typedef {{ slug: string, title: string }} TopicLink */

export default class {
  /** @type {TopicLink[]} */
  topics = []
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
    return slug ?? ''
  }

  @onMount
  trackNavigation() {
    // SPA navigations swap the slot content but keep this wrapper-page
    // component alive, so follow the runtime's navigation events to keep
    // the active topic highlighted.
    window.addEventListener('tachyon:navigate', (event) => {
      const detail = /** @type {CustomEvent<{ pathname?: string }>} */ (event).detail
      this.activeSlug = this.slugFrom(detail?.pathname ?? location.pathname)
    })
  }

  @onMount
  async loadTopics() {
    try {
      const response = await fetch('/shared/data/docs.json')
      const payload = await response.json()
      this.topics = payload.order.map((/** @type {string} */ slug) => ({
        slug,
        title: payload.topics[slug]?.title ?? slug,
      }))
    } catch {
      this.topics = []
    }
  }
}
