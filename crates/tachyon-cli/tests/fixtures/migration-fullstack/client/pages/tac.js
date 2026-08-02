// @ts-check

export default class {
  /** @type {string} */
  headline = 'Tachyon full-stack fixture'

  constructor() {
    if (typeof document !== 'undefined') document.title = 'Tachyon full-stack fixture'
  }
}
