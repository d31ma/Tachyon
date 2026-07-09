// @ts-check

/**
 * @typedef {{ heading: string, body: string, code?: string, id?: string }} TopicSection
 * @typedef {{ title: string, summary: string, sections: TopicSection[] }} Topic
 * @typedef {{
 *   order?: string[],
 *   groups?: { topics: string[] }[],
 *   topics: Record<string, Topic>
 * }} DocsPayload
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

  /** @returns {{ id: string, heading: string }[]} */
  get pageLinks() {
    return this.topic?.sections
      .filter((section) => section.id)
      .map((section) => ({ id: section.id ?? '', heading: section.heading })) ?? []
  }

  get slug() {
    if (typeof location === 'undefined') return ''
    const [, , slug] = location.pathname.split('/')
    return slug ?? ''
  }

  /** @param {string} value @returns {string} */
  slugify(value) {
    return value
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '') || 'section'
  }

  /** @param {Topic} topic @param {string} slug @returns {Topic & { slug: string }} */
  enrich(topic, slug) {
    /** @type {Map<string, number>} */
    const seen = new Map()
    const sections = topic.sections.map((section) => {
      const base = this.slugify(section.heading)
      const count = seen.get(base) ?? 0
      seen.set(base, count + 1)
      return {
        ...section,
        id: count ? `${base}-${count + 1}` : base,
      }
    })
    return { slug, ...topic, sections }
  }

  /** @param {string} id */
  scrollToSection(id) {
    if (typeof document === 'undefined') return
    const selector = `[data-doc-anchor="${id.replaceAll('"', '\\"')}"]`
    document.querySelector(selector)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    if (typeof history !== 'undefined') {
      history.replaceState(history.state, '', `#${id}`)
    }
  }

  @onMount
  async load() {
    try {
      const response = await fetch('/shared/data/docs.json', { cache: 'reload' })
      /** @type {DocsPayload} */
      const payload = await response.json()
      /** @type {string[]} */
      const order = payload.order ?? payload.groups?.flatMap((group) => group.topics) ?? Object.keys(payload.topics)
      const slug = this.slug
      const entry = payload.topics[slug]
      this.topic = entry ? this.enrich(entry, slug) : null
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
