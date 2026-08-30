// @ts-nocheck -- validated by the Tachyon compiler and website contract gates.
import { currentPath, features, groups } from '/shared/scripts/navigation.js'

export default class {
  static __tachyonOnMount = ['title']

  list() {
    return groups()
  }

  count() {
    return features().length
  }

  path() {
    return currentPath()
  }

  source() {
    return 'website/client/shared/data/features.json'
  }

  /** One entry per group, for the contents list beside the page. */
  contents() {
    return groups().map((group) => ({ heading: group.name, anchor: `#${group.slug}` }))
  }

  title() {
    document.title = 'Features — Tachyon docs'
  }
}
