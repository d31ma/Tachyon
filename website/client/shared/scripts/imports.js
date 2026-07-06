// Browser entry: Tachyon bundles this file and emits /imports.css from the
// CSS import below (site.css inlines the vendored DuVay stylesheet).
import '../styles/site.css'

// DuVay Light-DOM web components, vendored under /shared/assets/duvay so the
// site works without a CDN and inside `default-src 'self'` CSP.
if (!document.querySelector('script[data-duvay-wc]')) {
  const components = document.createElement('script')
  components.type = 'module'
  components.src = '/shared/assets/duvay/duvay-wc.min.js'
  components.dataset.duvayWc = 'true'
  document.head.appendChild(components)

  // DuVay behaviors: [w-theme-toggle] cycling + persistence, dropdowns.
  const behaviors = document.createElement('script')
  behaviors.defer = true
  behaviors.src = '/shared/assets/duvay/duvay.min.js'
  behaviors.dataset.duvayJs = 'true'
  document.head.appendChild(behaviors)
}

// Apply the persisted DuVay theme before first paint to avoid a flash.
// The site header's Tac companion owns toggling and persistence.
const storedTheme = localStorage.getItem('w-theme')
if (storedTheme) document.documentElement.setAttribute('w-theme', storedTheme)

// Native hosts: keep status-bar icon contrast in sync with the theme. The
// bridge is injected after page load, so poll briefly for it before the
// first sync.
const nativeWindow = /** @type {any} */ (window)
if (nativeWindow.__tcNativeHost__) {
  const syncStatusBar = () => {
    const dark = document.documentElement.getAttribute('w-theme') === 'dark'
    nativeWindow.__tcNativeBridge__
      ?.invoke('ui.statusBarStyle', { style: dark ? 'light-content' : 'dark-content' })
      .catch(() => {})
  }
  const awaitBridge = (tries = 20) => {
    if (nativeWindow.__tcNativeBridge__) syncStatusBar()
    else if (tries > 0) setTimeout(() => awaitBridge(tries - 1), 250)
  }
  awaitBridge()
  new MutationObserver(syncStatusBar).observe(document.documentElement, { attributeFilter: ['w-theme'] })
}

