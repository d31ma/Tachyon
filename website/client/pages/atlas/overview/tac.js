document.title = 'Overview — Tachyon capability atlas'

export function refreshAll() {
  window.dispatchEvent(new CustomEvent('tachyon:refresh'))
}
