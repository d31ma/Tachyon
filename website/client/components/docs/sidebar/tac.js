import { currentPath, groups, guides } from '/shared/scripts/navigation.js'

export default class {
  /**
   * Whether the drawer is showing.
   *
   * Only meaningful in the narrow layout, where the navigation has no column
   * of its own; the toggle that sets it is not rendered at any other width, so
   * a wide shell simply never reads this.
   */
  open = false

  toggle() {
    this.open = !this.open
  }

  close() {
    this.open = false
  }

  /** Escape closes it, which is what any overlay owes a keyboard. */
  dismiss(_event, key) {
    if (key === 'Escape') this.close()
  }

  /**
   * The sidebar's sections, each a group of links.
   *
   * Guides and features are one navigation rather than two lists stacked on
   * each other: a reader looking for "Browser storage" should find it in the
   * same place they found "Routing", and whether the page behind it was
   * authored as a guide or generated from the catalogue is not their problem.
   */
  sections() {
    const here = currentPath()
    const sections = [
      { name: 'Guide', slug: 'guide', links: guides().map(({ title, path }) => ({ title, path })) },
      ...groups().map((group) => ({
        name: group.name,
        slug: group.slug,
        links: group.features.map(({ title, path }) => ({ title, path })),
      })),
    ]
    const holds = (section) => section.links.some((link) => link.path === here)
    // Somewhere with no entry of its own — /docs itself, or the catalogue —
    // opens the guide rather than nothing. Every section shut is a navigation
    // showing no destinations at all, and on a narrow screen, where the
    // sidebar sits above the article, it is the first thing the reader meets.
    const orphan = !sections.some(holds)
    return sections.map((section, index) => ({
      ...section,
      links: section.links.map((link) => ({ ...link, current: link.path === here })),
      // Open the section holding the current page and leave the rest closed.
      // A <details> with no `open` collapses, so a reader arriving on one
      // feature is not handed forty-four links to scroll past.
      open: orphan ? index === 0 : holds(section),
    }))
  }
}
