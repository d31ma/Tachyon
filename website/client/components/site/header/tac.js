// @ts-check

// Keep this relative in authored source so the v26.30.04 bundler can resolve
// it. The Rust compiler rewrites this exact client/shared path to the stable
// `/shared/` output URL when it emits the island module.
import '../../../shared/scripts/imports.js'

export default class {
  /** @param {HTMLElement} root @param {AbortSignal} signal */
  hydrate(root, signal) {
    document.body.dataset.platform = 'web'
    document.body.dataset.os = 'web'

    const menu = root.querySelector('#mobile-menu')
    const trigger = root.querySelector('[aria-controls="mobile-menu"]')
    if (!(menu instanceof HTMLElement) || !(trigger instanceof HTMLElement)) return
    trigger.setAttribute('aria-expanded', String(menu.classList.contains('open')))
    menu.addEventListener('click', (event) => {
      if (!(event.target instanceof Element) || !event.target.closest('[href]')) return
      menu.classList.remove('open')
      trigger.setAttribute('aria-expanded', 'false')
    }, { signal })
  }
}
