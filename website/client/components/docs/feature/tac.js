// @ts-nocheck -- validated by the Tachyon compiler and website contract gates.
import { currentPath, currentSlug, feature, slugify } from '/shared/scripts/navigation.js'

export default class {
  static __tachyonOnMount = ['title']

  /** Shown when the id names no feature, rather than an empty article. */
  static NOT_FOUND = {
    id: '',
    title: 'Feature not found',
    summary: 'That feature does not exist. Browse the catalogue instead.',
    group: '',
    files: [],
  }

  get id() {
    return currentSlug()
  }

  path() {
    return currentPath()
  }

  source() {
    return 'website/client/shared/data/features.json'
  }

  entry() {
    return feature(this.id) ?? this.constructor.NOT_FOUND
  }

  /** The group this belongs to, linked back into the catalogue. */
  groupAnchor() {
    return `/docs/features#${slugify(this.entry().group)}`
  }

  files() {
    return this.entry().files ?? []
  }

  title() {
    document.title = `${this.entry().title} — Tachyon docs`
  }
}
