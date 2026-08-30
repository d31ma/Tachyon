// @ts-nocheck -- validated by the Tachyon compiler and website contract gates.
import { currentPath, currentSlug, guide, slugify } from '/shared/scripts/navigation.js'

export default class {
  static __tachyonOnMount = ['title']

  /** Shown when the slug names no guide, rather than an empty article. */
  static NOT_FOUND = {
    title: 'Topic not found',
    summary: 'That guide does not exist. Pick one from the list.',
    sections: [],
  }

  /** The in-page anchor a heading gets, in one place so both lists agree. */
  static anchorFor(heading) {
    return `#${slugify(heading)}`
  }

  /**
   * Derived rather than stored, because the first render happens before mount:
   * a slug assigned in a lifecycle method would arrive after the article had
   * already been rendered with an empty one.
   */
  get slug() {
    return currentSlug()
  }

  path() {
    return currentPath()
  }

  /** The file a reader would edit, which is the data rather than the route. */
  source() {
    return 'website/client/shared/data/docs.json'
  }

  topic() {
    return guide(this.slug) ?? this.constructor.NOT_FOUND
  }

  sections() {
    return (this.topic().sections ?? []).map((section) => ({
      ...section,
      anchor: slugify(section.heading),
      // A single-file snippet reuses the tabbed component with one tab, so
      // every code block behaves the same way.
      single: section.code ? [{ name: section.language ?? 'example', code: section.code }] : undefined,
    }))
  }

  contents() {
    return (this.topic().sections ?? []).map((section) => ({
      heading: section.heading,
      anchor: this.constructor.anchorFor(section.heading),
    }))
  }

  title() {
    document.title = `${this.topic().title} — Tachyon docs`
  }
}
