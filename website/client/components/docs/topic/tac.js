// @ts-check

/**
 * @typedef {{ heading: string, body: string, code?: string }} TopicSection
 * @typedef {{ title: string, summary: string, sections: TopicSection[] }} Topic
 */

export default class {
  /** @type {(Topic & { slug: string }) | null} */
  topic = null
  /** @type {{ slug: string, title: string } | null} */
  previous = null
  /** @type {{ slug: string, title: string } | null} */
  next = null
  /** @type {boolean} */
  loading = true

  get slug() {
    if (typeof location === 'undefined') return ''
    const [, , slug] = location.pathname.split('/')
    return slug ?? ''
  }

  @onMount
  async load() {
    try {
      const response = await fetch('/shared/data/docs.json')
      const payload = await response.json()
      /** @type {string[]} */
      const order = payload.order
      const slug = this.slug
      const entry = payload.topics[slug]
      this.topic = entry ? { slug, ...entry } : null
      const index = order.indexOf(slug)
      const linkFor = (/** @type {number} */ at) => {
        const other = order[at]
        return other ? { slug: other, title: payload.topics[other]?.title ?? other } : null
      }
      this.previous = index > 0 ? linkFor(index - 1) : null
      this.next = index >= 0 ? linkFor(index + 1) : null
      if (this.topic && typeof document !== 'undefined') {
        document.title = `${this.topic.title} — Tachyon docs`
      }
    } catch {
      this.topic = null
    } finally {
      this.loading = false
    }
  }
}
