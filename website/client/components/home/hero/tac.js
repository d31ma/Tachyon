// @ts-check

export default class {
  /** @type {number} */
  $visits = 0
  /** @type {number} */
  $$totalVisits = 0
  /** @type {string} */
  installCommand = 'curl -fsSL https://tachyon.del.ma/install.sh | sh\nty init my-app && cd my-app\nty serve'

  constructor() {
    this.$visits += 1
    this.$$totalVisits += 1
  }
}
