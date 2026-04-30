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
}

document.documentElement.setAttribute('data-theme', 'light')

// Exposes `window.fylo` as a global Proxy so any script (Tac companion or plain
// <script>) can query collections via property access - no manual fetch wiring.
//
// Usage:
//   await fylo.users.find({ $ops: [{ role: { $eq: 'admin' } }] })
//   await fylo.users.get('usr_xxx')
//   await fylo.users.patch('usr_xxx', { role: 'admin' })   // requires YON_DATA_BROWSER_READONLY=false
//   await fylo.users.del('usr_xxx')                         // requires YON_DATA_BROWSER_READONLY=false
//   await fylo.sql('SELECT * FROM users LIMIT 10')
//   await fylo.collections()                                // list all collections
//
// Reserved property names (cannot be used as collection names): sql, collections,
// enabled, root. Any other string property returns a per-collection proxy.
const __fyloMeta = /** @type {HTMLMetaElement | null} */ (document.querySelector('meta[name="fylo-browser-path"]'))
const __fyloBP = __fyloMeta?.content || '/_fylo'

/**
 * @param {string} path
 * @param {Record<string, unknown>} body
 */
async function __fyloPostJson(path, body) {
  const r = await fetch(`${__fyloBP}${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  return r.json()
}

/** @param {string} collection */
function __fyloCollection(collection) {
  return {
    /** @param {Record<string, unknown>} [query] */
    async find(query = {}) {
      return __fyloPostJson('/api/query', { kind: 'find', collection, query })
    },
    /** @param {number} [limit] */
    async list(limit = 25) {
      const r = await fetch(`${__fyloBP}/api/docs?collection=${encodeURIComponent(collection)}&limit=${limit}`)
      return r.json()
    },
    /** @param {string} id */
    async get(id) {
      const r = await fetch(`${__fyloBP}/api/doc?collection=${encodeURIComponent(collection)}&id=${encodeURIComponent(id)}`)
      return r.json()
    },
    /** @param {number} [since] */
    async events(since = 0) {
      const r = await fetch(`${__fyloBP}/api/events?collection=${encodeURIComponent(collection)}&since=${since}`)
      return r.json()
    },
    /** @param {string} id @param {Record<string, unknown>} doc */
    async patch(id, doc) {
      return __fyloPostJson('/api/patch', { collection, id, doc })
    },
    /** @param {string} id */
    async del(id) {
      const r = await fetch(`${__fyloBP}/api/delete?collection=${encodeURIComponent(collection)}&id=${encodeURIComponent(id)}`, { method: 'DELETE' })
      return r.json()
    },
    async rebuild() {
      return __fyloPostJson('/api/rebuild', { collection })
    },
  }
}

const __fyloState = {
  enabled: false,
  /** @type {string | undefined} */
  root: undefined,
  /** @param {string} source */
  sql(source) {
    return __fyloPostJson('/api/query', { kind: 'sql', source })
  },
  async collections() {
    const r = await fetch(`${__fyloBP}/api/collections`, { cache: 'reload' })
    if (!r.ok) return { root: '', collections: [] }
    const d = await r.json()
    __fyloState.root = d.root
    return d
  },
  async meta() {
    const r = await fetch(`${__fyloBP}/api/meta`, { cache: 'reload' })
    if (!r.ok) return null
    return r.json()
  },
}

window.fylo = /** @type {any} */ (new Proxy(__fyloState, {
  get(target, prop) {
    if (typeof prop !== 'string') return Reflect.get(target, prop)
    if (prop in target) return Reflect.get(target, prop)
    return __fyloCollection(prop)
  },
  set(target, prop, value) {
    return Reflect.set(target, prop, value)
  },
  has(target, prop) {
    return typeof prop === 'string' || prop in target
  },
}))

// Probe meta once; if the browser route is mounted, flip enabled and cache the root.
fetch(`${__fyloBP}/api/meta`, { cache: 'reload' })
  .then(r => r.ok ? r.json() : null)
  .then(meta => {
    if (meta) {
      __fyloState.enabled = true
      __fyloState.root = meta.root
    }
  })
  .catch(() => { /* fylo browser not mounted; leave disabled */ })
