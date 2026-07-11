// @ts-check

export default class extends Tac {
  /** @type {string} */
  @publish('release')
  release = 'Tac + Yon'

  @onMount
  stampPlatform() {
    // Dogfood the platform-aware Tac globals: the shell adapts per
    // platform (desktop / mobile / web) through CSS hooks on <body>, with
    // the concrete environment/os (windows/macos/linux/android/ios/web)
    // alongside. Theme cycling is DuVay's own [w-theme-toggle] behavior, so
    // the header never rerenders and its light-DOM components stay upgraded.
    document.body.dataset.platform = this.tac.platform.platform
    document.body.dataset.os = this.tac.platform.os
    this.closeMobileMenuAfterNavigation()
  }

  closeMobileMenuAfterNavigation() {
    const menu = document.getElementById('mobile-menu')
    const trigger = document.querySelector('[aria-controls="mobile-menu"]')
    if (!menu || !trigger) return
    if (!trigger.hasAttribute('aria-expanded')) trigger.setAttribute('aria-expanded', 'false')

    menu.addEventListener('click', (event) => {
      if (!(event.target instanceof Element)) return
      if (!event.target.closest('[href]')) return
      menu.classList.remove('open')
      trigger.setAttribute('aria-expanded', 'false')
    })
  }
}
