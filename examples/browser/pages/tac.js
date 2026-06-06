// @ts-check

export default class extends Tac {
  /** @type {boolean} */
  $sidebarOpen = true
  /** @type {string} */
  $theme = 'light'
  /** @type {number} */
  $visits = 0
  /** @type {number} */
  $$totalVisits = 0
  /** @type {string} */
  @publish
  release = 'TACHYON'
  /** @type {string} */
  headline = 'Build what the browser can imagine, backed by Yon and FYLO'
  /** @type {string} */
  subtitle = 'A living application tour of reactive interfaces, native web capabilities, polyglot endpoints, durable data and observable production flows.'

  get sidebarLabel() {
    return this.$sidebarOpen ? 'Hide menu' : 'Show menu'
  }

  /** @returns {void} */
  toggleSidebar() {
    this.$sidebarOpen = !this.$sidebarOpen
  }

  /** @returns {void} */
  @subscribe('tachyon:toggle-theme')
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
  @publish('tachyon:refresh')
  @onMount
  async refreshAll() {
    this.$visits += 1
    this.$$totalVisits += 1
    document.title = 'TACHYON Showcase'
    this.applyTheme()
  }

  /** @param {string} id */
  scrollToSection(id) {
    const el = document.getElementById(id)
    if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  @onMount
  adaptSidebarToViewport() {
    const media = window.matchMedia('(max-width: 900px)')
    const sync = () => {
      this.$sidebarOpen = !media.matches
    }
    sync()
    media.addEventListener('change', sync)
  }
}
