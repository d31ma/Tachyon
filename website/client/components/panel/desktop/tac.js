// @ts-check

export default class {
  /** @type {string} */
  workerReport = 'asking the worker…'

  @onMount
  async probeWorker() {
    try {
      const response = await fetch('tac://language/rust', { cache: 'reload' })
      const payload = await response.json()
      this.workerReport = `a ${payload.method} request of ${payload.result} bytes (counted inside tac.wasm)`
    } catch {
      this.workerReport = 'worker unavailable in this environment'
    }
  }
}
