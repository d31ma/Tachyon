// @ts-check

/**
 * @typedef {{ heading?: string, body?: string, code?: string }} TopicSection
 * @typedef {{ slug: string, title: string, summary: string, sections: TopicSection[] }} TopicLink
 * @typedef {{ title: string, summary: string, topics: TopicLink[] }} TopicGroup
 * @typedef {{
 *   order?: string[],
 *   groups?: { title: string, summary?: string, topics: string[] }[],
 *   topics: Record<string, { title?: string, summary?: string, sections?: TopicSection[] }>
 * }} DocsPayload
 */

export default class {
  /** @type {TopicGroup[]} */
  groups = []
  /** @type {string} */
  activeSlug = ''
  /** @type {string} */
  query = ''

  constructor() {
    if (typeof location !== 'undefined') {
      this.activeSlug = this.slugFrom(location.pathname)
    }
  }

  /** @returns {TopicGroup[]} */
  get visibleGroups() {
    const query = this.query.trim().toLowerCase()
    if (!query) return this.groups

    return this.groups
      .map((group) => ({
        ...group,
        topics: group.topics.filter((topic) => this.matches(topic, query)),
      }))
      .filter((group) => group.topics.length > 0)
  }

  /** @param {string} pathname @returns {string} */
  slugFrom(pathname) {
    const [, , slug] = pathname.split('/')
    return slug ?? ''
  }

  /** @param {TopicLink} topic @param {string} query @returns {boolean} */
  matches(topic, query) {
    const searchable = [
      topic.title,
      topic.summary,
      ...topic.sections.flatMap((section) => [section.heading, section.body, section.code]),
    ]
      .filter(Boolean)
      .join(' ')
      .toLowerCase()
    return searchable.includes(query)
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
      const response = await fetch('/shared/data/docs.json', { cache: 'reload' })
      /** @type {DocsPayload} */
      const payload = await response.json()
      const order = payload.order ?? Object.keys(payload.topics)
      const topicFor = (/** @type {string} */ slug) => {
        const topic = payload.topics[slug] ?? {}
        return {
          slug,
          title: topic.title ?? slug,
          summary: topic.summary ?? '',
          sections: topic.sections ?? [],
        }
      }
      const sourceGroups = payload.groups?.length
        ? payload.groups
        : [{ title: 'Documentation', summary: 'Guides and reference.', topics: order }]
      this.groups = sourceGroups.map((group) => ({
        title: group.title,
        summary: group.summary ?? '',
        topics: group.topics.map(topicFor).filter((topic) => Boolean(topic.title)),
      }))
    } catch {
      this.groups = []
    }
  }
}
