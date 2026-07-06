// @ts-check

export default class {
  /** @type {number} */
  $visits = 0
  /** @type {number} */
  $$totalVisits = 0
  /** @type {string} */
  installCommand = 'bun add @d31ma/tachyon\nyon.init my-app && cd my-app\nbun run serve'

  constructor() {
    this.$visits += 1
    this.$$totalVisits += 1
  }
}
