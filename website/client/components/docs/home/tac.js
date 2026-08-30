// @ts-nocheck -- validated by the Tachyon compiler and website contract gates.
import { currentPath, features, groups, guides } from '/shared/scripts/navigation.js'

export default class {
  static __tachyonOnMount = ['title']

  topics() {
    return guides()
  }

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
    return 'website/client/shared/data/docs.json'
  }

  contents() {
    return [
      { heading: 'Guides', anchor: '#guides' },
      { heading: 'Features', anchor: '#features' },
    ]
  }

  title() {
    document.title = 'Documentation — Tachyon'
  }
}
