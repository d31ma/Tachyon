// @ts-check

export default class {
  /** @type {number} */
  $visits = 0
  /** @type {number} */
  $$totalVisits = 0
  /** @type {string} */
  installCommand = 'curl -fsSL https://tachyon.del.ma/install.sh | sh\nty init my-app && cd my-app\nty serve'

  constructor() {
    try {
      this.$visits = Number(sessionStorage.getItem('tachyon-home-visits') ?? 0) + 1
      this.$$totalVisits = Number(localStorage.getItem('tachyon-home-total-visits') ?? 0) + 1
      sessionStorage.setItem('tachyon-home-visits', String(this.$visits))
      localStorage.setItem('tachyon-home-total-visits', String(this.$$totalVisits))
    } catch {
      this.$visits += 1
      this.$$totalVisits += 1
    }
  }

  /** @returns {void} */
  hydrate() {}
}
