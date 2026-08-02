// @ts-check

/**
 * @typedef {{ heading: string, body: string, code?: string, id?: string }} TopicSection
 * @typedef {{ title: string, summary: string, sections: TopicSection[] }} Topic
 * @typedef {{ order?: string[], groups?: { topics: string[] }[], topics: Record<string, Topic> }} DocsPayload
 */

/** @param {string} tag @param {string} className @param {string} [text] */
function element(tag, className, text) {
  const node = document.createElement(tag)
  if (className) node.className = className
  if (text !== undefined) node.textContent = text
  return node
}

export default class {
  /** @type {(Topic & { slug: string }) | null} */
  topic = null
  /** @type {{ slug: string, title: string } | null} */
  previous = null
  /** @type {{ slug: string, title: string } | null} */
  next = null
  /** @type {HTMLElement | null} */
  root = null

  get slug() {
    const [, , slug] = location.pathname.split('/')
    return slug ?? ''
  }

  /** @param {string} value @returns {string} */
  slugify(value) {
    return value.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'section'
  }

  /** @param {Topic} topic @param {string} slug @returns {Topic & { slug: string }} */
  enrich(topic, slug) {
    /** @type {Map<string, number>} */
    const seen = new Map()
    const sections = topic.sections.map((section) => {
      const base = this.slugify(section.heading)
      const count = seen.get(base) ?? 0
      seen.set(base, count + 1)
      return { ...section, id: count ? `${base}-${count + 1}` : base }
    })
    return { slug, ...topic, sections }
  }

  /** @param {string} id */
  scrollToSection(id) {
    this.root?.querySelector(`[data-doc-anchor="${CSS.escape(id)}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    history.replaceState(history.state, '', `#${id}`)
  }

  /** @param {HTMLElement} root @param {AbortSignal} signal */
  async hydrate(root, signal) {
    this.root = root.querySelector('.topic')
    try {
      const response = await fetch('/shared/data/docs.json', { cache: 'reload', signal })
      /** @type {DocsPayload} */
      const payload = await response.json()
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
      if (this.topic) document.title = `${this.topic.title} — Tachyon docs`
    } catch (error) {
      if (!signal.aborted) console.warn('Unable to load this documentation topic.', error)
      this.topic = null
    }
    if (!signal.aborted) this.render(signal)
  }

  /** @param {AbortSignal} signal */
  render(signal) {
    if (!this.root) return
    this.root.replaceChildren()
    if (!this.topic) {
      const empty = document.createElement('w-empty-state')
      empty.setAttribute('title', 'Topic not found')
      empty.setAttribute('text', 'Pick a topic from the documentation navigation.')
      this.root.append(empty)
      return
    }

    const header = element('header', 'topic-header')
    header.append(
      element('p', 'topic-kicker', 'Guide'),
      element('h1', '', this.topic.title),
      element('p', 'topic-summary', this.topic.summary),
    )

    const main = element('div', 'topic-main')
    for (const section of this.topic.sections) {
      const sectionNode = element('section', 'topic-section')
      sectionNode.dataset.docAnchor = section.id ?? ''
      sectionNode.append(element('h2', '', section.heading), element('p', '', section.body))
      if (section.code) {
        const pre = element('pre', 'code-block')
        pre.append(element('code', '', section.code))
        sectionNode.append(pre)
      }
      main.append(sectionNode)
    }

    const pager = element('nav', 'topic-pager')
    pager.setAttribute('aria-label', 'Topic pagination')
    if (this.previous) pager.append(this.pagerLink(this.previous, 'Previous', false))
    pager.append(element('span', 'topic-spacer'))
    if (this.next) pager.append(this.pagerLink(this.next, 'Next', true))
    main.append(pager)

    const toc = element('aside', 'topic-toc')
    toc.setAttribute('aria-label', 'On this page')
    toc.append(element('p', 'topic-toc-title', 'On this page'))
    const list = element('ul', 'topic-toc-list')
    for (const section of this.topic.sections) {
      const item = element('li', 'topic-toc-item')
      const button = /** @type {HTMLButtonElement} */ (element('button', '', section.heading))
      button.type = 'button'
      button.addEventListener('click', () => this.scrollToSection(section.id ?? ''), { signal })
      item.append(button)
      list.append(item)
    }
    toc.append(list)
    this.root.append(header, main, toc)
  }

  /** @param {{ slug: string, title: string }} topic @param {string} relation @param {boolean} next */
  pagerLink(topic, relation, next) {
    const link = /** @type {HTMLAnchorElement} */ (element('a', `topic-pager-link${next ? ' topic-pager-next' : ''}`))
    link.href = `/docs/${topic.slug}`
    link.append(element('span', 'topic-pager-rel', relation), element('span', 'topic-pager-title', topic.title))
    return link
  }
}
