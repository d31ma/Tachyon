// @ts-check

export default class {
  /** @type {string} */
  companionReport = 'the companion ABI is ready'

  @onMount
  reportCompanion() {
    const { platform, os } = this.tac.platform
    this.companionReport = `${platform} platform controller on the ${os} environment`
  }
}
