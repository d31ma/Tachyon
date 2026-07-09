// @ts-check

export default class extends Tac {
  /** @type {string} */
  @publish('release')
  release = 'Tac + Yon'

  @onMount
  stampPlatform() {
    // Dogfood the platform-aware Tac globals: the shell adapts per
    // environment (browser / desktop / mobile) through CSS hooks on <body>.
    // Theme cycling is DuVay's own [w-theme-toggle] behavior, so the header
    // never rerenders and its light-DOM components stay upgraded.
    document.body.dataset.environment = this.tac.platform.environment
    document.body.dataset.platform = this.tac.platform.platform
    this.closeMobileMenuAfterNavigation()
  }

  closeMobileMenuAfterNavigation() {
    const menu = document.getElementById('mobile-menu')
    const trigger = document.querySelector('[aria-controls="mobile-menu"]')
    if (!menu || !trigger) return
    if (!trigger.hasAttribute('aria-expanded')) trigger.setAttribute('aria-expanded', 'false')

    menu.addEventListener('click', (event) => {
      if (!(event.target instanceof Element)) return
      if (!event.target.closest('a[href]')) return
      menu.classList.remove('open')
      trigger.setAttribute('aria-expanded', 'false')
    })
  }
}
