// @ts-nocheck -- validated by the Tachyon compiler and website contract gates.
import { neighbours } from '/shared/scripts/navigation.js'

export default class {
  /**
   * `path` arrives as a property rather than being read from `location`: a
   * page knows which page it is, and deriving it here would make the pager
   * disagree with the article above it on any route that is not the last
   * segment of its own URL.
   *
   * @type {string | undefined}
   */

  #around() {
    return neighbours(this.path ?? '')
  }

  previous() {
    return this.#around().previous
  }

  next() {
    return this.#around().next
  }
}
