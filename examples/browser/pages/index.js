// @ts-check

export default class extends Tac {
  /** @type {boolean} */
  $sidebarOpen = true
  /** @type {string} */
  $theme = 'light'
  /** @type {number} */
  $visits = 0
  /** @type {string} */
  @provide('demo-release')
  release = 'TACHYON'
  /** @type {string} */
  headline = 'One app, two layers, every route talking live'
  /** @type {string} */
  subtitle = 'Tac drives the browser experience while Yon executes file-routed backend handlers in multiple languages.'

  /** @returns {void} */
  toggleSidebar() {
    this.$sidebarOpen = !this.$sidebarOpen
  }

  /** @returns {void} */
  toggleTheme() {
    this.$theme = this.$theme === 'dark' ? 'light' : 'dark'
    this.applyTheme()
  }

  /** @returns {void} */
  applyTheme() {
    document.documentElement.setAttribute('data-theme', this.$theme)
    document.documentElement.style.colorScheme = this.$theme
  }

  /** @returns {Promise<void>} */
  @onMount
  async refreshAll() {
    this.$visits += 1
    document.title = 'TACHYON Showcase'
    this.applyTheme()
    window.dispatchEvent(new CustomEvent('tachyon:refresh'))
  }

  @onMount
  bindThemeListener() {
    window.addEventListener('tachyon:toggle-theme', () => { this.toggleTheme() })
  }
}
