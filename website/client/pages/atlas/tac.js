// @ts-check

// The page root holds no reactive state, so the shell renders once per
// navigation. Live state lives in the panel components — the re-render
// boundary Tac recommends.
export default class {
  /** @type {string} */
  subtitle = 'A living tour of reactive interfaces, native web capabilities, polyglot Wasm workers, durable browser data and observable client flows — with no server behind it.'

  sections = [
    { id: 'compose', index: '01', label: 'Compose' },
    { id: 'react', index: '02', label: 'React' },
    { id: 'connect', index: '03', label: 'Connect' },
    { id: 'store', index: '04', label: 'Store' },
    { id: 'observe', index: '05', label: 'Observe' },
    { id: 'extend', index: '06', label: 'Extend' },
  ]

  constructor() {
    if (typeof document !== 'undefined') document.title = 'Capability atlas — Tachyon'
  }

  /** @returns {void} */
  @publish('tachyon:refresh')
  @onMount
  refreshAll() {}

  /** @param {string} id */
  scrollToSection(id) {
    const el = document.getElementById(id)
    if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }
}
