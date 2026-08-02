// Browser entry. The Rust compiler emits the shared stylesheet separately and
// this module installs the vendored DuVay components and behaviours.

// The v26.30.04 Bun bundler consumes this import into its generated CSS. The
// Rust publisher links the same file from HTML and removes only this bare CSS
// import from the emitted browser module.
import '../styles/site.css'

// DuVay Light-DOM web components, vendored under /shared/assets/duvay so the
// site works without a CDN and inside `default-src 'self'` CSP.
// The v26.30.04 compiler evaluates imported companion modules while it builds
// with a deliberately tiny document stub. Keep browser installation inert in
// that environment; the same module runs normally when the emitted page loads.
const hasBrowserDocument = typeof document !== 'undefined'
  && typeof document.querySelector === 'function'

if (hasBrowserDocument && !document.querySelector('script[data-duvay-wc]')) {
  const components = document.createElement('script')
  components.type = 'module'
  components.src = new URL('../assets/duvay/duvay-wc.min.js', import.meta.url).href
  components.dataset.duvayWc = 'true'
  document.head.appendChild(components)

  // DuVay behaviors: [w-theme-toggle] cycling + persistence, dropdowns.
  const behaviors = document.createElement('script')
  behaviors.defer = true
  behaviors.src = new URL('../assets/duvay/duvay.min.js', import.meta.url).href
  behaviors.dataset.duvayJs = 'true'
  document.head.appendChild(behaviors)
}

// Apply the persisted DuVay theme before first paint to avoid a flash.
// The site header's Tac companion owns toggling and persistence.
if (hasBrowserDocument) {
  const storedTheme = localStorage.getItem('w-theme')
  if (storedTheme) document.documentElement.setAttribute('w-theme', storedTheme)
  window.addEventListener('storage', (event) => {
    if (event.key !== 'w-theme') return
    document.documentElement.setAttribute('w-theme', event.newValue || 'light')
  })

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
}
