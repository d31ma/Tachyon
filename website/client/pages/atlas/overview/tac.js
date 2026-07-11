// @ts-check

export default class {
  /** @type {string} */
  subtitle = 'A living tour of reactive interfaces, native web capabilities, polyglot companions, durable browser data and observable client flows — with no server behind it. Pick a section from the sidebar; every panel runs in your browser.'

  constructor() {
    if (typeof document !== 'undefined') document.title = 'Overview — Tachyon capability atlas'
  }

  /** @returns {void} */
  @publish('tachyon:refresh')
  @onMount
  refreshAll() {}
}
