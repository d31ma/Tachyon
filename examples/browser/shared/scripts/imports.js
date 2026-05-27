// Example shell bootstrap: load local design assets and expose the FYLO browser helper.
if (!document.querySelector('link[data-demo-style]')) {
  const fonts = document.createElement('link')
  fonts.rel = 'stylesheet'
  fonts.href = 'https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600;700&family=IBM+Plex+Mono:wght@500;600&display=swap'
  fonts.dataset.demoStyle = 'true'
  document.head.appendChild(fonts)

  const appStyles = document.createElement('link')
  appStyles.rel = 'stylesheet'
  appStyles.href = '/shared/assets/brand.css?v=' + Date.now()
  appStyles.dataset.demoStyle = 'true'
  document.head.appendChild(appStyles)

  const tailwind = document.createElement('script')
  tailwind.src = 'https://cdn.tailwindcss.com'
  tailwind.dataset.demoStyle = 'true'
  document.head.appendChild(tailwind)

  const tailwindConfig = document.createElement('script')
  tailwindConfig.textContent = `
    tailwind.config = {
      theme: {
        extend: {
          fontFamily: {
            sans: ['"IBM Plex Sans"', '-apple-system', 'BlinkMacSystemFont', '"Segoe UI"', 'system-ui', 'sans-serif'],
            mono: ['"IBM Plex Mono"', 'monospace'],
          },
          colors: {
            primary: 'var(--md-sys-color-primary)',
            'on-primary': 'var(--md-sys-color-on-primary)',
            'primary-container': 'var(--md-sys-color-primary-container)',
            'on-primary-container': 'var(--md-sys-color-on-primary-container)',
            secondary: 'var(--md-sys-color-secondary)',
            'on-secondary': 'var(--md-sys-color-on-secondary)',
            'secondary-container': 'var(--md-sys-color-secondary-container)',
            'on-secondary-container': 'var(--md-sys-color-on-secondary-container)',
            tertiary: 'var(--md-sys-color-tertiary)',
            'on-tertiary': 'var(--md-sys-color-on-tertiary)',
            'tertiary-container': 'var(--md-sys-color-tertiary-container)',
            'on-tertiary-container': 'var(--md-sys-color-on-tertiary-container)',
            error: 'var(--md-sys-color-error)',
            'on-error': 'var(--md-sys-color-on-error)',
            'error-container': 'var(--md-sys-color-error-container)',
            'on-error-container': 'var(--md-sys-color-on-error-container)',
            background: 'var(--md-sys-color-background)',
            'on-background': 'var(--md-sys-color-on-background)',
            surface: 'var(--md-sys-color-surface)',
            'on-surface': 'var(--md-sys-color-on-surface)',
            'surface-variant': 'var(--md-sys-color-surface-variant)',
            'on-surface-variant': 'var(--md-sys-color-on-surface-variant)',
            outline: 'var(--md-sys-color-outline)',
            'outline-variant': 'var(--md-sys-color-outline-variant)',
            'surface-container-lowest': 'var(--md-sys-color-surface-container-lowest)',
            'surface-container-low': 'var(--md-sys-color-surface-container-low)',
            'surface-container': 'var(--md-sys-color-surface-container)',
            'surface-container-high': 'var(--md-sys-color-surface-container-high)',
            'surface-container-highest': 'var(--md-sys-color-surface-container-highest)',
          },
          borderRadius: {
            none: '0',
            DEFAULT: '0',
            sm: '0',
            md: '0',
            lg: '0',
            xl: '0',
            '2xl': '0',
            '3xl': '0',
            full: '0',
          },
        },
      },
    }
  `
  document.head.appendChild(tailwindConfig)

  const materialScript = document.createElement('script')
  materialScript.type = 'module'
  materialScript.src = 'https://esm.sh/@material/web/all.js?bundle'
  document.head.appendChild(materialScript)
}

document.documentElement.setAttribute('data-theme', 'light')

// The Fylo browser client is now provided by the framework runtime
// (src/runtime/fylo-global.js) which self-bootstraps `window.fylo` from the
// `<meta name="fylo-browser-path">` tag injected into every Tachyon shell.
//
// Usage (available globally in any script or Tac companion):
//   await fylo.users.find({ $ops: [{ role: { $eq: 'admin' } }] })
//   await fylo.users.get('usr_xxx')
//   await fylo.users.patch('usr_xxx', { role: 'admin' })   // requires YON_DATA_BROWSER_READONLY=false
//   await fylo.users.del('usr_xxx')                         // requires YON_DATA_BROWSER_READONLY=false
//   await fylo.sql('SELECT * FROM users LIMIT 10')
//   await fylo.collections()                                // list all collections
//   fylo.setCredentials('user', 'pass')                     // set Basic Auth for API calls
//   fylo.clearCredentials()                                 // remove stored auth
