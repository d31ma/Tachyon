export default class {
  /**
   * `entries` arrives as a property and is deliberately not declared: a
   * property is only assigned when the instance does not already carry that
   * name, so declaring it would shadow whatever the page passed in.
   *
   * @type {{ heading: string, anchor: string }[] | undefined}
   */

  list() {
    return Array.isArray(this.entries) ? this.entries : []
  }

  worthShowing() {
    return this.list().length > 1
  }
}
