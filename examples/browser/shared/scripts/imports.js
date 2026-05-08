// Example shell bootstrap: load local design assets and expose the FYLO browser helper.
if (!document.querySelector('link[data-demo-style]')) {
  const fonts = document.createElement('link')
  fonts.rel = 'stylesheet'
  fonts.href = 'https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600;700&family=IBM+Plex+Mono:wght@500;600&display=swap'
  fonts.dataset.demoStyle = 'true'
  document.head.appendChild(fonts)

  const appStyles = document.createElement('link')
  appStyles.rel = 'stylesheet'
  appStyles.href = '/shared/assets/brand.css'
  appStyles.dataset.demoStyle = 'true'
  document.head.appendChild(appStyles)

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
