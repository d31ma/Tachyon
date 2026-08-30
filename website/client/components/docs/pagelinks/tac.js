import { currentPath } from '/shared/scripts/navigation.js'

export default class {
  static REPOSITORY = 'https://github.com/d31ma/Tachyon'

  /**
   * `source` arrives as a property: the repository-relative file a reader
   * would actually edit. Every guide is one entry in a data file rather than
   * a page of prose, so "edit this page" has to name that file — pointing at
   * a route would send them somewhere no text lives.
   *
   * @type {string | undefined}
   */

  editHref() {
    const file = this.source || 'website/client/shared/data/docs.json'
    return `${this.constructor.REPOSITORY}/edit/main/${file}`
  }

  reportHref() {
    const title = encodeURIComponent(`Docs: ${currentPath()}`)
    return `${this.constructor.REPOSITORY}/issues/new?title=${title}`
  }
}
