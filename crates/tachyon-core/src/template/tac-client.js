const planElement = document.getElementById('tachyon-view')
if (!planElement) throw new Error('Tac client view plan is missing.')
const plan = JSON.parse(planElement.textContent || '{}')
const instances = new Map()
const componentRecords = new Map()
const lifecycleStarted = new Set()
const controllers = new Map()
const mountSchedules = new Map()
const cancelMount = (key) => mountSchedules.get(key)?.cancel()
let pageModule = {}
let pageOwner = Object.create(null)
let rendering = null
let renderAgain = false
let renderedNodes = 0
let renderedIterations = 0
let renderedInstances = new Set()
let renderedElements = new Map()
let renderedMounts = []
const elementRecords = new WeakMap()
const eventListeners = new WeakMap()
let nativeWrites = Promise.resolve()
let nativeWriteCount = 0
const nativeMethods = new WeakSet()

// Interpret the compiler's bounded expression AST. Authored expression source
// never reaches the browser and is never passed to eval or Function.
const evaluate = async (node, environment) => {
  if (!node || typeof node !== 'object') return undefined
  switch (node.k) {
    case 'await': return await evaluate(node.e, environment)
    case 'lit': return node.v
    case 'id': {
      if (node.n in environment.locals) {
        return environment.locals[node.n]
      }
      if (environment.owner && node.n in environment.owner) return environment.owner[node.n]
      return pageModule[node.n]
    }
    case 'get': {
      const target = await evaluate(node.o, environment)
      return target === null || target === undefined ? undefined : target[node.p]
    }
    case 'idx': {
      const target = await evaluate(node.o, environment)
      return Array.isArray(target) ? target[node.i] : undefined
    }
    case 'not': return !await evaluate(node.e, environment)
    case 'cmp': {
      const left = await evaluate(node.l, environment)
      const right = await evaluate(node.r, environment)
      if (node.op === 'eq') return left === right
      if (node.op === 'ne') return left !== right
      if (node.op === 'lt') return left < right
      if (node.op === 'le') return left <= right
      if (node.op === 'gt') return left > right
      return left >= right
    }
    case 'log': {
      const left = await evaluate(node.l, environment)
      if (node.op === 'and') return left ? await evaluate(node.r, environment) : left
      return left || await evaluate(node.r, environment)
    }
    case 'num': {
      const left = await evaluate(node.l, environment)
      const right = await evaluate(node.r, environment)
      if (node.op === 'add') return left + right
      if (node.op === 'sub') return left - right
      if (node.op === 'mul') return left * right
      return left / right
    }
    case 'if':
      return await evaluate(node.c, environment)
        ? await evaluate(node.t, environment)
        : await evaluate(node.f, environment)
    case 'call': {
      const callee = node.c
      const owner = callee.k === 'get' || callee.k === 'idx'
        ? await evaluate(callee.o, environment)
        : environment.owner
      const target = await evaluate(callee, environment)
      if (typeof target !== 'function') return undefined
      const arguments_ = await Promise.all((node.a || []).map((value) => evaluate(value, environment)))
      return target.apply(owner, arguments_)
    }
    default: return undefined
  }
}

const truthy = (value) => Boolean(value)
const display = (value) => {
  if (value === null || value === undefined || typeof value === 'object') return ''
  return String(value)
}
const childEnvironment = (environment, locals, owner = environment.owner) => ({
  owner,
  locals: Object.assign(Object.create(environment.locals), locals),
})
const ownedEnvironment = (owner, locals = {}) => ({ owner, locals: Object.assign(Object.create(null), locals) })

const resolveEventArgument = async (argument, event, environment) => {
  if ('value' in argument) return argument.value
  if ('expression' in argument) return evaluate(argument.expression, environment)
  if (!('event' in argument) || argument.event === '') return event
  let value = event
  for (const segment of String(argument.event).split('.')) {
    if (value === null || value === undefined) return undefined
    value = value[segment]
  }
  return value
}

const bindEvent = (element, type, binding, environment) => {
  const listener = async (event) => {
    try {
      const arguments_ = await Promise.all(
        (binding.arguments || []).map((argument) => resolveEventArgument(argument, event, environment)),
      )
      if (binding.assign) {
        const { target, operator } = binding.assign
        const current = environment.owner?.[target]
        const value = arguments_[0]
        environment.owner[target] =
          operator === '+=' ? current + value
          : operator === '-=' ? current - value
          : operator === '*=' ? current * value
          : operator === '/=' ? current / value
          : value
        await render()
      } else {
        const ownerHandler = environment.owner?.[binding.handler]
        const handler = ownerHandler ?? pageModule[binding.handler]
        if (typeof handler !== 'function') {
          throw new TypeError(`Tac handler '${binding.handler}' is not available.`)
        }
        // Browser handlers keep their implicit DOM event. A native ABI call
        // receives only authored JSON arguments, never an implicit DOM object.
        await handler.call(environment.owner, ...(nativeMethods.has(handler) ? arguments_ : [event, ...arguments_]))
        if (ownerHandler) await render()
      }
      document.documentElement.dataset.tachyonEvents = 'handled'
    } catch {
      document.documentElement.dataset.tachyonEventError = 'handler_failed'
    }
  }
  element.addEventListener(type, listener)
  const listeners = eventListeners.get(element) || []
  listeners.push({ type, listener })
  eventListeners.set(element, listeners)
}

const applyAttributes = async (element, attributes, environment) => {
  for (const attribute of attributes || []) {
    if (attribute.event) {
      bindEvent(element, attribute.eventType, attribute.event, environment)
      continue
    }
    const value = 'expression' in attribute
      ? await evaluate(attribute.expression, environment)
      : attribute.value
    if (value === false || value === null || value === undefined) {
      element.removeAttribute(attribute.name)
      continue
    }
    const rendered = value === true ? '' : display(value)
    element.setAttribute(attribute.name, rendered)
    if (['checked', 'disabled', 'selected'].includes(attribute.name)) element[attribute.name] = Boolean(value)
    else if (attribute.name === 'value' && 'value' in element) {
      // A custom element may expose `value` as a getter only, and assigning to
      // one throws hard enough to abort the whole render. The attribute is
      // already set above, which is what such an element actually reads.
      try { element.value = rendered } catch { /* read-only property */ }
    }
  }
}

// ── Browser storage ───────────────────────────────────────────────────────
// Three caches, each used for what the platform built it for: CacheStorage
// holds Request/Response pairs for static assets, IndexedDB holds structured
// API responses, and Web Storage holds small persisted view fields. Every one
// of them is optional at runtime — a browser in private mode, with storage
// denied, or out of quota must degrade to plain network behavior, never throw
// into a render.

const STATIC_CACHE_PREFIX = 'tachyon-static-'

// The offline cache belongs to the service worker, whose name carries the
// build digest. That digest is computed over the emitted output, so it cannot
// be embedded here without making the digest depend on itself; the live name
// is discovered instead. Before the worker's first activation there is nothing
// to warm, and it caches on demand anyway.
const openStaticCache = async () => {
  try {
    if (!globalThis.caches) return null
    const name = (await caches.keys()).find((value) => value.startsWith(STATIC_CACHE_PREFIX))
    return name ? await caches.open(name) : null
  } catch { return null }
}

// Warms the offline cache with assets this view can still reach. A component
// mounted on visibility or interaction is never fetched until it is needed, so
// without this it is missing from the cache exactly when the network is gone.
const precacheAssets = async (urls) => {
  const cache = await openStaticCache()
  if (!cache) return
  if (!Array.isArray(urls) || urls.length > 512) throw new RangeError('Tac precache exceeds 512 assets.')
  const deadline = Date.now() + 30000
  let warmedBytes = 0
  for (const raw of new Set(urls)) {
    if (Date.now() >= deadline || warmedBytes >= 33554432) break
    try {
      const url = new URL(raw, location.href)
      if (url.origin !== location.origin || !['http:', 'https:'].includes(url.protocol)) continue
      if (await cache.match(url)) continue
      const response = await fetch(url, { credentials: 'omit', signal: AbortSignal.timeout(5000) })
      if (response.ok && response.type === 'basic'
        && !/(?:no-store|private)/i.test(response.headers.get('cache-control') || '')
        && !response.headers.get('vary')) {
        const body = await responseBody(response, 4194304)
        if (body === null) continue
        if (warmedBytes + body.byteLength > 33554432) break
        warmedBytes += body.byteLength
        await cache.put(url, new Response(body, { status: response.status, statusText: response.statusText, headers: response.headers }))
      }
    } catch { /* the worker will fetch it on demand */ }
  }
}

const clearStaticCache = async () => {
  try {
    if (!globalThis.caches) return
    for (const name of await caches.keys()) {
      if (name.startsWith(STATIC_CACHE_PREFIX)) await caches.delete(name)
    }
  } catch { /* best effort */ }
}

const planAssets = (nodes, found = []) => {
  for (const node of nodes || []) {
    if (found.length >= 512) break
    if (node.k === 'component') {
      if (node.module) found.push(node.module)
      planAssets(node.template, found)
      planAssets(node.slot, found)
    }
    planAssets(node.children, found)
  }
  return found
}

const FETCH_DATABASE = 'tachyon-fetch-cache'
// Discard caches created before anonymous-only persistence was enforced.
const FETCH_DATABASE_VERSION = 2
const FETCH_STORE = 'responses'
const CACHE_POLICIES = new Set(['cache-first', 'network-first', 'reload', 'no-store'])
const MAX_RESPONSE_BYTES = 262144
const MAX_CACHE_ENTRIES = 128
const MAX_CACHE_AGE = 86400000
let fetchDatabase

const openFetchDatabase = () => {
  if (fetchDatabase !== undefined) return fetchDatabase
  fetchDatabase = new Promise((resolve) => {
    let settled = false
    const finish = (value) => {
      if (settled) { value?.close(); return }
      settled = true
      clearTimeout(timer)
      resolve(value)
    }
    const timer = setTimeout(() => finish(null), 2000)
    let request
    try {
      if (typeof indexedDB === 'undefined') { finish(null); return }
      request = indexedDB.open(FETCH_DATABASE, FETCH_DATABASE_VERSION)
    }
    catch { finish(null); return }
    request.onupgradeneeded = () => {
      if (request.result.objectStoreNames.contains(FETCH_STORE)) request.result.deleteObjectStore(FETCH_STORE)
      if (!request.result.objectStoreNames.contains(FETCH_STORE)) {
        request.result.createObjectStore(FETCH_STORE, { keyPath: 'key' })
      }
    }
    request.onsuccess = () => {
      request.result.onversionchange = () => request.result.close()
      finish(request.result)
    }
    request.onerror = () => finish(null)
    request.onblocked = () => finish(null)
  })
  return fetchDatabase
}

const fetchStore = async (mode) => {
  const database = await openFetchDatabase()
  if (!database) return null
  try { return database.transaction(FETCH_STORE, mode).objectStore(FETCH_STORE) }
  catch { return null }
}

const cacheHeadersAllowed = (headers) => !headers.has('vary') && !headers.has('content-range')
  && !/(?:^|,)\s*(?:private|no-store)\b/i.test(headers.get('cache-control') || '')

const readCachedResponse = async (key) => {
  const store = await fetchStore('readonly')
  if (!store) return null
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(null), 2000)
    const finish = (value) => { clearTimeout(timer); resolve(value) }
    let request
    try { request = store.get(key) } catch { finish(null); return }
    request.onsuccess = () => {
      const entry = request.result
      if (!entry || entry.status === 206 || !Number.isFinite(entry.updatedAt) || Date.now() - entry.updatedAt > MAX_CACHE_AGE
        || entry.body?.byteLength > MAX_RESPONSE_BYTES) { finish(null); return }
      try {
        const headers = new Headers(entry.headers)
        if (!cacheHeadersAllowed(headers)) { finish(null); return }
      } catch { finish(null); return }
      // A 204 or 304 rejects any body at all, so an empty one stays null.
      const body = entry.body?.byteLength ? new Uint8Array(entry.body) : null
      try {
        finish(new Response(body, {
          status: entry.status,
          statusText: entry.statusText,
          headers: entry.headers,
        }))
      } catch { finish(null) }
    }
    request.onerror = () => finish(null)
  })
}

const responseBody = async (response, limit = MAX_RESPONSE_BYTES) => {
  if (!response.body) return new ArrayBuffer(0)
  const reader = response.body.getReader()
  const chunks = []
  let length = 0
  let timedOut = false
  const timer = setTimeout(() => { timedOut = true; void reader.cancel().catch(() => {}) }, 2000)
  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      length += value.byteLength
      if (length > limit) { void reader.cancel().catch(() => {}); return null }
      chunks.push(value)
    }
    if (timedOut) return null
    const body = new Uint8Array(length)
    let offset = 0
    for (const chunk of chunks) { body.set(chunk, offset); offset += chunk.byteLength }
    return body.buffer
  } finally { clearTimeout(timer) }
}

const mutateCache = async (change) => {
  const store = await fetchStore('readwrite')
  if (!store) return
  await new Promise((resolve) => {
    const timer = setTimeout(() => { try { store.transaction.abort() } catch {} resolve() }, 2000)
    const finish = () => { clearTimeout(timer); resolve() }
    store.transaction.oncomplete = finish
    store.transaction.onabort = finish
    store.transaction.onerror = finish
    try { change(store) } catch { finish() }
  })
}

const writeCachedResponse = async (key, response) => {
  let body
  try { body = await responseBody(response) } catch { return }
  if (body === null) return
  await mutateCache((store) => {
    const count = store.count()
    count.onsuccess = () => {
      // Clear a full cache atomically; no unbounded cursor scan or eviction queue.
      if (count.result >= MAX_CACHE_ENTRIES) store.clear()
    store.put({
      key,
      status: response.status,
      statusText: response.statusText,
      headers: Object.fromEntries(response.headers.entries()),
      body,
      updatedAt: Date.now(),
    })
    }
  })
}

const deleteCachedResponse = async (key) => {
  await mutateCache((store) => store.delete(key))
}

const deleteCachedPrefix = async (prefix) => {
  if (typeof IDBKeyRange === 'undefined') return
  await mutateCache((store) => store.delete(IDBKeyRange.bound(prefix, `${prefix}\uffff`)))
}

const invalidateCache = async (keys = [], prefixes = []) => {
  if (!Array.isArray(keys) || !Array.isArray(prefixes)
    || [...keys, ...prefixes].some((key) => typeof key !== 'string' || key.length > 4096)) {
    throw new TypeError('Tac invalidation keys must be bounded strings.')
  }
  if (keys.length + prefixes.length > 128) throw new RangeError('Tac invalidation exceeds 128 keys.')
  await Promise.all([
    ...[...keys].map((key) => deleteCachedResponse(key)),
    ...[...prefixes].map((prefix) => deleteCachedPrefix(prefix)),
  ])
}

const fetchRequestCanPersist = (request, policy) => {
  const method = request.method.toUpperCase()
  // Only a read is addressed by its URL alone, so only a read is cacheable.
  // Ranges name a different representation and never share this full-response key.
  // Never persist ambient cookies, authorization, caller no-store, or cross-origin
  // responses. Caching authenticated reads requires an application-owned policy.
  return (method === 'GET' || method === 'HEAD') && policy !== 'no-store'
    && request.credentials === 'omit' && !request.headers.has('authorization')
    && !request.headers.has('range') && !request.headers.has('if-range')
    && new URL(request.url).origin === location.origin && request.cache !== 'no-store'
}

const fetchCacheKey = (request, policy, configuredKey) => {
  const key = fetchRequestCanPersist(request, policy) ? configuredKey ?? `fetch:${request.method.toUpperCase()}:${request.url}` : null
  if (key !== null && (typeof key !== 'string' || key.length > 4096)) throw new TypeError('Invalid Tac cache key.')
  return key
}

const responseCanPersist = (response) => response.ok && response.status !== 206
  && !response.redirected && response.type !== 'opaque' && cacheHeadersAllowed(response.headers)

const fetchCacheFallback = async (key, request, policy, error) => {
  // 'reload' and 'no-store' asked for the network specifically, so its
  // failure is theirs to see. The rest fall back to what was stored.
  if (key && policy !== 'reload' && !request.signal.aborted) {
    const cached = await readCachedResponse(key)
    if (cached) return cached
  }
  throw error
}

// Fetch with an explicit caching policy. The default is network-first: a view
// asking for data means current data, and the cache is what answers when the
// network cannot. Only a caller that knows its resource is immutable should
// ask for cache-first.
const tacFetch = async (input, init = {}, options = {}) => {
  const policy = options.cache ?? 'network-first'
  if (!CACHE_POLICIES.has(policy)) throw new TypeError(`Unknown Tac cache policy '${policy}'.`)
  const request = new Request(typeof input === 'string' ? new URL(input, location.href) : input, init)
  request.signal.throwIfAborted()
  const key = fetchCacheKey(request, policy, options.key)

  if (key && policy === 'cache-first') {
    const cached = await readCachedResponse(key)
    request.signal.throwIfAborted()
    if (cached) return cached
  }
  try {
    // The IndexedDB policy owns this cache decision; the browser HTTP cache
    // must not turn network-first, reload, or no-store into an implicit hit.
    const response = await fetch(new Request(request, { cache: 'no-store' }))
    if (key && responseCanPersist(response)) await writeCachedResponse(key, response.clone())
    else if (key) await deleteCachedResponse(key)
    // A write that succeeded is what makes a read stale, so this is where
    // the caller's invalidation runs.
    if (!key && response.ok) await invalidateCache(options.invalidateKeys, options.invalidatePrefixes)
    return response
  } catch (error) {
    return fetchCacheFallback(key, request, policy, error)
  }
}

// A `$`-prefixed field persists for the tab, `$$` persists across sessions.
// Reading storage can throw outright in a sandboxed frame, so even acquiring
// it is guarded.
const webStorage = (persistent) => {
  try { return persistent ? localStorage : sessionStorage } catch { return null }
}

const readStoredField = (store, key, fallback) => {
  try {
    const raw = store.getItem(key)
    return raw === null || raw.length > 65536 || new TextEncoder().encode(raw).byteLength > 65536 ? fallback : JSON.parse(raw)
  } catch { return fallback }
}

const writeStoredField = (store, key, value) => {
  try {
    if (value === undefined) store.removeItem(key)
    else {
      const raw = JSON.stringify(value)
      if (raw?.length <= 65536 && new TextEncoder().encode(raw).byteLength <= 65536) store.setItem(key, raw)
    }
  } catch { /* private mode, or over quota */ }
}

const boundOwners = new WeakSet()

// The stored value wins over the declared one, which is what makes it a
// default rather than a reset. Nothing is written until a field is assigned,
// so changing a default in source still reaches a returning visitor.
const bindPersistentFields = (owner, scope) => {
  if (boundOwners.has(owner)) return owner
  boundOwners.add(owner)
  for (const name of Object.keys(owner)) {
    if (!name.startsWith('$') || !Object.getOwnPropertyDescriptor(owner, name)?.configurable) continue
    const store = webStorage(name.startsWith('$$'))
    if (!store) continue
    const key = `tac:${scope}:${name}`
    let current = readStoredField(store, key, owner[name])
    Object.defineProperty(owner, name, {
      configurable: true,
      enumerable: true,
      get: () => current,
      set: (value) => { current = value; writeStoredField(store, key, value) },
    })
  }
  return owner
}

// Two instances of one component on one page share a scope unless they are
// told apart, which is what `persist-id` is for.
const persistScope = (id, properties) =>
  `${id}:${properties?.persistId ?? properties?.['persist-id'] ?? location.pathname}`

const importAsset = (raw) => {
  const url = new URL(raw, location.href)
  const web = url.origin === location.origin && url.pathname.startsWith('/.tachyon/components/')
  const native = url.origin === location.origin && url.pathname.includes('/WebBundle/tachyon-runtime/components/')
  if (!web && !native) throw new TypeError('Tac component asset is not a generated same-origin URL.')
  return import(url.href)
}


const componentOwner = async (node, properties, path) => {
  renderedInstances.add(path)
  if (instances.has(path)) return instances.get(path)
  let owner = properties
  if (node.module) {
    const module = await importAsset(node.module)
    if (typeof module.default !== 'function') throw new TypeError(`Tac component '${node.name}' must export a default class.`)
    owner = new module.default(properties)
  }
  attachRuntime(owner, persistScope(node.name, properties))
  instances.set(path, owner)
  componentRecords.set(path, { node, properties, owner })
  return owner
}

const snapshotState = (owner) => {
  const state = {}
  for (const [name, value] of Object.entries(owner || {})) {
    if (typeof value !== 'function') state[name] = value
  }
  try { return structuredClone(state) } catch { return {} }
}

const snapshotMutableDom = () => ({
  active: document.activeElement?.id || null,
  elements: [...document.querySelectorAll('[id]')].slice(0, 2048).map((element) => ({
    id: element.id,
    open: element instanceof HTMLDetailsElement ? element.open : undefined,
    value: element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement ? element.value : undefined,
    checked: element instanceof HTMLInputElement ? element.checked : undefined,
    scroll: element.scrollLeft || element.scrollTop ? [element.scrollLeft, element.scrollTop] : undefined,
  })),
})
const restoreMutableDom = (state) => {
  for (const saved of state.elements) {
    const element = document.getElementById(saved.id)
    if (!element) continue
    if (saved.open !== undefined && element instanceof HTMLDetailsElement) element.open = saved.open
    if (saved.value !== undefined && (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement)) element.value = saved.value
    if (saved.checked !== undefined && element instanceof HTMLInputElement) element.checked = saved.checked
    if (saved.scroll) element.scrollTo(saved.scroll[0], saved.scroll[1])
  }
  if (state.active) document.getElementById(state.active)?.focus({ preventScroll: true })
}

const hotUpdate = async (boundaries, version) => {
  const selected = new Set(boundaries || [])
  const domState = snapshotMutableDom()
  // The replaced modules are about to be re-imported under a new query, but a
  // cached copy of the old bytes would outlive them under the original URL.
  await clearStaticCache()
  for (const [path, record] of componentRecords) {
    if (!selected.has(record.node.name)) continue
    const state = typeof record.owner.hotState === 'function'
      ? structuredClone(await record.owner.hotState())
      : snapshotState(record.owner)
    const asset = record.node.module
    if (!asset) continue
    const url = new URL(asset, location.href)
    url.searchParams.set('tachyon_hot', version || String(Date.now()))
    const module = await importAsset(url.href)
    if (typeof module.default !== 'function') throw new TypeError(`Tac component '${record.node.name}' must export a default class.`)
    const owner = new module.default(record.properties)
    if (typeof record.owner.hotDispose === 'function') await record.owner.hotDispose()
    disposeRuntime(record.owner)
    if (typeof owner.restoreHotState === 'function') await owner.restoreHotState(state)
    else Object.assign(owner, state)
    attachRuntime(owner, persistScope(record.node.name, record.properties))
    controllers.get(path)?.abort()
    cancelMount(path)
    controllers.delete(path)
    lifecycleStarted.delete(path)
    record.owner = owner
    instances.set(path, owner)
  }
  await render()
  restoreMutableDom(domState)
}

const mountComponent = (host, owner, policy, key) => {
  const previous = mountSchedules.get(key)
  if (previous?.host === host && previous.owner === owner && previous.policy === policy) return
  cancelMount(key)
  if (!policy || policy === 'never') return
  const pending = { host, owner, policy, cancelled: false, cleanup: () => {}, cancel: () => {} }
  const release = () => {
    pending.cleanup()
    pending.cleanup = () => {}
    if (mountSchedules.get(key) === pending) mountSchedules.delete(key)
  }
  pending.cancel = () => { pending.cancelled = true; release() }
  mountSchedules.set(key, pending)
  const stillOwned = () => host.isConnected && componentRecords.get(key)?.owner === owner
  const activate = async () => {
    if (pending.cancelled || !stillOwned()) { pending.cancel(); return false }
    release()
    try {
      controllers.get(key)?.abort()
      const controller = new AbortController()
      controllers.set(key, controller)
      host.tachyonComponent = { instance: owner, refresh: render, controller }
      if (typeof owner.mount === 'function') await owner.mount(host, controller.signal)
      // Compatibility only: this runs after browser rendering and adopts no SSR.
      else if (typeof owner.hydrate === 'function') await owner.hydrate(host, controller.signal)
      if (controller.signal.aborted || !stillOwned()) return false
      if (!lifecycleStarted.has(key)) {
        lifecycleStarted.add(key)
        for (const method of mountMethods(owner)) {
          if (typeof owner[method] === 'function') await owner[method]()
          if (controller.signal.aborted || !stillOwned()) return false
        }
      }
      host.dataset.tachyonActive = 'true'
      host.removeAttribute('data-tachyon-mount-error')
      return true
    } catch {
      host.dataset.tachyonActive = 'false'
      host.dataset.tachyonMountError = 'activation_failed'
      return false
    }
  }
  if (policy === 'idle') {
    if (globalThis.requestIdleCallback) {
      const id = requestIdleCallback(activate)
      pending.cleanup = () => globalThis.cancelIdleCallback?.(id)
    } else {
      const id = setTimeout(activate, 1)
      pending.cleanup = () => clearTimeout(id)
    }
  }
  else if (policy === 'visible' && globalThis.IntersectionObserver) {
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) { observer.disconnect(); void activate() }
    }, { rootMargin: '100px' })
    pending.cleanup = () => observer.disconnect()
    observer.observe(host)
  } else if (policy === 'interaction') {
    const listener = async (event) => {
      if (event.cancelable) event.preventDefault()
      event.stopImmediatePropagation()
      host.removeEventListener('pointerdown', listener, true)
      host.removeEventListener('keydown', listener, true)
      const target = event.target
      if (await activate()) {
        target.dispatchEvent(new Event(event.type, { bubbles: true, cancelable: true }))
      }
    }
    host.addEventListener('pointerdown', listener, true)
    host.addEventListener('keydown', listener, true)
    pending.cleanup = () => {
      host.removeEventListener('pointerdown', listener, true)
      host.removeEventListener('keydown', listener, true)
    }
  } else queueMicrotask(activate)
}

const renderText = async (parts, environment) => {
  let value = ''
  for (const part of parts || []) {
    value += 'expression' in part ? display(await evaluate(part.expression, environment)) : part.value
  }
  return document.createTextNode(value)
}

const recordElement = (element, node, environment, path) => {
  const locals = []
  for (const name in environment.locals) locals.push([name, environment.locals[name]])
  elementRecords.set(element, { node, owner: environment.owner, path,
    locals, children: [...element.childNodes],
    attributes: new Map(element.getAttributeNames().map((name) => [name, element.getAttribute(name)])) })
  renderedElements.set(path, element)
  return element
}

const sameLocals = (previous, next) => previous.length === next.length
  && previous.every(([name, value], index) => next[index][0] === name && Object.is(next[index][1], value))

const retainedPair = (old, freshChild) => {
  const previous = elementRecords.get(old)
  const fresh = previous && renderedElements.get(previous.path)
  const next = fresh && elementRecords.get(fresh)
  if (!next || previous.node !== next.node || previous.owner !== next.owner) return null
  if (old.tagName !== fresh.tagName || !sameLocals(previous.locals, next.locals)) return null
  if (freshChild && freshChild.parentNode !== fresh) return null
  return { old, fresh, previous }
}

const focusChain = (active, fragment) => {
  const chain = []
  let old = active
  let freshChild = null
  while (old && old !== document.body) {
    // Third-party light-DOM wrappers are checked separately during preflight.
    if (!elementRecords.has(old) && chain.length) {
      old = old.parentElement
      continue
    }
    const pair = retainedPair(old, freshChild)
    if (!pair) return []
    chain.unshift(pair)
    freshChild = pair.fresh
    old = old.parentElement
  }
  if (old !== document.body || chain[0]?.fresh.parentNode !== fragment) return []
  return chain
}

const ownedSlots = (parent) => [...parent.querySelectorAll('slot')].filter((slot) => {
  for (let node = slot.parentElement; node && node !== parent; node = node.parentElement) {
    if (elementRecords.has(node)) return false
  }
  return true
})

const distributionGroups = (parent, source, focusedChild) => {
  const slots = ownedSlots(parent)
  const groups = new Map(slots.map((slot) => [slot.getAttribute('name') || '', { slot, children: [] }]))
  if (!slots.includes(focusedChild.parentNode) || groups.size !== slots.length) return null
  for (const child of [...source.childNodes]) {
    const name = child.nodeType === Node.ELEMENT_NODE ? child.getAttribute('slot') || '' : ''
    const group = groups.get(name)
    if (!group) return null
    group.children.push(child)
  }
  return groups
}

const changesObservedAttribute = ({ old, fresh, previous }) => {
  if (!old.localName.includes('-')) return false
  try {
    const observed = old.constructor.observedAttributes || []
    if (!Array.isArray(observed) || observed.length > 256) return true
    const attributes = elementRecords.get(fresh).attributes
    return observed.some((name) => previous.attributes.get(name) !== attributes.get(name))
  } catch { return true }
}

// Finish every compatibility check before changing any connected node. Custom
// observed-attribute setters may replace their entire implementation subtree.
const preflightFocus = (chain, fragment) => {
  const distributions = new Map()
  for (let depth = 0; depth < chain.length; depth += 1) {
    const pair = chain[depth]
    if (changesObservedAttribute(pair)) return null
    const parent = depth ? chain[depth - 1].old : document.body
    if (pair.old.parentNode === parent) continue
    const source = depth ? chain[depth - 1].fresh : fragment
    const groups = depth ? distributionGroups(parent, source, pair.old) : null
    if (!groups) return null
    distributions.set(pair, groups)
  }
  return distributions
}

const replaceAndRefocus = (fragment, active, replacement) => {
  const selection = [active.selectionStart, active.selectionEnd, active.selectionDirection]
  document.body.replaceChildren(fragment)
  replacement.focus({ preventScroll: true })
  if (selection[0] !== null && selection[0] !== undefined) {
    try { replacement.setSelectionRange(...selection) } catch { /* non-text control */ }
  }
}

const syncAuthoredAttributes = (element, previous, next) => {
  for (const [name] of previous.attributes) {
    if (!next.attributes.has(name)) element.removeAttribute(name)
  }
  for (const [name, value] of next.attributes) {
    // Preserve classes/attributes added by a custom element implementation.
    if (previous.attributes.get(name) !== value) element.setAttribute(name, value)
  }
}

const changedDuringRender = (element, active, editingAtStart) => element === active
  && editingAtStart?.element === element && editingAtStart.value !== element.value

const syncControlValue = (element, fresh, next, active, editingAtStart) => {
  if (!next.attributes.has('value') || !('value' in element)) return
  if (changedDuringRender(element, active, editingAtStart) || element.value === fresh.value) return
  try { element.value = fresh.value } catch { /* custom read-only value */ }
}

const syncControlFlags = (element, fresh, previous, next) => {
  for (const name of ['checked', 'disabled', 'selected']) {
    if (previous.attributes.has(name) || next.attributes.has(name)) element[name] = fresh[name]
  }
}

const transferEventListeners = (element, fresh) => {
  for (const { type, listener } of eventListeners.get(element) || []) element.removeEventListener(type, listener)
  const listeners = eventListeners.get(fresh) || []
  for (const { type, listener } of listeners) element.addEventListener(type, listener)
  eventListeners.set(element, listeners)
}

class FocusedBodyUpdate {
  constructor(chain, distributions, active, editingAtStart) {
    Object.assign(this, { chain, distributions, active, editingAtStart })
  }

  syncElement(element, fresh) {
    const previous = elementRecords.get(element)
    const next = elementRecords.get(fresh)
    syncAuthoredAttributes(element, previous, next)
    syncControlValue(element, fresh, next, this.active, this.editingAtStart)
    syncControlFlags(element, fresh, previous, next)
    transferEventListeners(element, fresh)
    elementRecords.set(element, next)
    if (fresh.tachyonComponent) element.tachyonComponent = fresh.tachyonComponent
  }

  commit(parent, source, depth) {
    const pair = this.chain[depth]
    if (!pair) {
      // Do not replace an active editing buffer or its composition text.
      if (!parent.matches('input, textarea, [contenteditable]')) parent.replaceChildren(...source.childNodes)
      return
    }
    if (pair.old.parentNode !== parent) {
      this.commitSlots(pair, depth)
      return
    }
    this.commitSiblings(parent, source, pair, depth)
  }

  commitSlots(pair, depth) {
    const owner = this.chain[depth - 1]
    for (const { slot, children } of this.distributions.get(pair).values()) {
      if (slot === pair.old.parentNode) {
        const content = document.createDocumentFragment()
        content.append(...children)
        this.commit(slot, content, depth)
      } else if (children.length || owner.previous.children.some((child) => child.parentNode === slot)) {
        slot.replaceChildren(...children)
      }
    }
  }

  commitSiblings(parent, source, pair, depth) {
    for (const child of [...parent.childNodes]) if (child !== pair.old) child.remove()
    let before = true
    for (const child of [...source.childNodes]) {
      if (child === pair.fresh) {
        this.syncElement(pair.old, pair.fresh)
        this.commit(pair.old, pair.fresh, depth + 1)
        before = false
      } else if (before) parent.insertBefore(child, pair.old)
      else parent.append(child)
    }
  }

  finalize() {
    const retained = new Map(this.chain.map((pair) => [pair.fresh, pair.old]))
    for (const { old, fresh } of this.chain) {
      const record = elementRecords.get(old)
      record.children = elementRecords.get(fresh).children.map((child) => retained.get(child) || child)
    }
    renderedMounts = renderedMounts.map(([host, ...arguments_]) => [retained.get(host) || host, ...arguments_])
  }
}

// Retain the connected editing chain only after a read-only preflight. A
// different lexical row/branch never inherits the old row's focus or state.
const commitBody = (fragment, editingAtStart) => {
  const active = document.activeElement
  const chain = focusChain(active, fragment)
  if (!chain.length) { document.body.replaceChildren(fragment); return }
  const distributions = preflightFocus(chain, fragment)
  if (!distributions) { replaceAndRefocus(fragment, active, chain.at(-1).fresh); return }
  const update = new FocusedBodyUpdate(chain, distributions, active, editingAtStart)
  update.commit(document.body, fragment, 0)
  update.finalize()
}

const renderComponent = async (node, environment, path) => {
  const properties = {}
  for (const property of node.properties || []) {
    if (property.event) continue
    properties[property.name] = 'expression' in property
      ? await evaluate(property.expression, environment)
      : property.value
  }
  const componentKey = `${path}:${node.name}`
  const owner = await componentOwner(node, properties, componentKey)
  for (const [name, value] of Object.entries(properties)) {
    if (!(name in owner)) owner[name] = value
  }
  const host = document.createElement('tachyon-component')
  host.dataset.tachyonComponent = node.name
  host.dataset.tachyonMount = node.mount || 'none'
  if (node.scope) host.setAttribute('data-tac-scope', node.name)
  const componentEnvironment = ownedEnvironment(owner)
  host.append(await renderNodes(node.template || [], componentEnvironment, `${path}.template`, {
    nodes: node.slot || [],
    environment,
  }))
  host.tachyonComponent = { instance: owner, refresh: render }
  renderedMounts.push([host, owner, node.mount, componentKey])
  return recordElement(host, node, environment, path)
}

const renderNode = async (node, environment, path, slot) => {
  renderedNodes += 1
  if (renderedNodes > 100000) throw new RangeError('Tac client view exceeds 100,000 nodes.')
  if (node.k === 'text') return renderText(node.parts, environment)
  if (node.k === 'comment') return document.createComment(node.value || '')
  if (node.k === 'slot') {
    return slot ? renderNodes(slot.nodes, slot.environment, `${path}.slot`, null) : document.createDocumentFragment()
  }
  if (node.k === 'iteration') {
    const fragment = document.createDocumentFragment()
    const values = await evaluate(node.iterable, environment)
    if (!Array.isArray(values)) return fragment
    if (values.length > 10000) throw new RangeError('Tac iteration exceeds 10,000 items.')
    for (let index = 0; index < values.length; index += 1) {
      if (++renderedIterations > 100000) throw new RangeError('Tac client view exceeds 100,000 iterations.')
      const local = childEnvironment(environment, { [node.binding]: values[index], $index: index })
      fragment.append(await renderNodes(node.children || [], local, `${path}.${index}`, slot))
    }
    return fragment
  }
  if (node.k === 'counted') {
    const fragment = document.createDocumentFragment()
    const from = Number(await evaluate(node.from, environment))
    const to = Number(await evaluate(node.to, environment))
    // The step is a magnitude and the comparison carries the direction, so the
    // two cannot disagree. The compiler already rejected a loop stepping away
    // from its limit; this guard is for what only goes wrong at run time — a
    // step of zero, or a NaN from a field that was not set.
    const magnitude = Number(await evaluate(node.step, environment))
    const ascending = node.comparison === 'lt' || node.comparison === 'le'
    const step = ascending ? magnitude : -magnitude
    if (!Number.isFinite(from) || !Number.isFinite(to) || !Number.isFinite(step) || magnitude <= 0) return fragment
    const passes = node.comparison === 'lt' ? (i) => i < to
      : node.comparison === 'le' ? (i) => i <= to
      : node.comparison === 'gt' ? (i) => i > to
      : (i) => i >= to
    let index = 0
    for (let counter = from; passes(counter); counter += step) {
      if (++renderedIterations > 100000) throw new RangeError('Tac client view exceeds 100,000 iterations.')
      if (index >= 10000) throw new RangeError('Tac iteration exceeds 10,000 items.')
      const local = childEnvironment(environment, { [node.binding]: counter, $index: index })
      fragment.append(await renderNodes(node.children || [], local, `${path}.${index}`, slot))
      index += 1
      if (counter + step === counter) throw new RangeError('Tac iteration step makes no progress.')
    }
    return fragment
  }
  if (node.k === 'switch') {
    const value = await evaluate(node.value, environment)
    let fallback
    for (const candidate of node.children || []) {
      if (candidate.k !== 'case') continue
      if (!candidate.when) fallback = candidate
      else if (value === await evaluate(candidate.when, environment)) {
        return renderNodes(candidate.children || [], environment, `${path}.case`, slot)
      }
    }
    return fallback
      ? renderNodes(fallback.children || [], environment, `${path}.default`, slot)
      : document.createDocumentFragment()
  }
  if (node.k === 'component') return renderComponent(node, environment, path)
  if (node.k !== 'element') return document.createDocumentFragment()
  const element = document.createElement(node.tag)
  await applyAttributes(element, node.attributes, environment)
  if (!node.void) element.append(await renderNodes(node.children || [], environment, `${path}.children`, slot))
  return recordElement(element, node, environment, path)
}

const renderNodes = async (nodes, environment, path, slot) => {
  const fragment = document.createDocumentFragment()
  for (let index = 0; index < nodes.length; index += 1) {
    const node = nodes[index]
    if (node.k === 'conditional') {
      if (node.branch !== 'if') continue
      let cursor = index
      let selected
      while (cursor < nodes.length) {
        const candidate = nodes[cursor]
        if (cursor > index && (candidate.k === 'comment'
          || (candidate.k === 'text' && candidate.parts?.every((part) => 'value' in part && !part.value.trim())))) {
          cursor += 1
          continue
        }
        if (candidate.k !== 'conditional') break
        if (!candidate.condition || truthy(await evaluate(candidate.condition, environment))) {
          selected = candidate
          cursor += 1
          break
        }
        cursor += 1
      }
      if (selected) fragment.append(await renderNodes(selected.children || [], environment, `${path}.${index}.branch`, slot))
      index = cursor - 1
      continue
    }
    fragment.append(await renderNode(node, environment, `${path}.${index}`, slot))
  }
  return fragment
}

const planElementByTag = (nodes, tag) => nodes.find((node) => node.k === 'element' && node.tag === tag)
const renderDocument = async (environment) => {
  renderedNodes = 0
  renderedIterations = 0
  renderedInstances = new Set()
  renderedElements = new Map()
  renderedMounts = []
  const editingAtStart = { element: document.activeElement, value: document.activeElement?.value }
  const htmlPlan = planElementByTag(plan.nodes || [], 'html')
  const headPlan = htmlPlan && planElementByTag(htmlPlan.children || [], 'head')
  const bodyPlan = htmlPlan && planElementByTag(htmlPlan.children || [], 'body')
  const runtimeAssets = [...document.head.querySelectorAll('[data-tachyon-runtime]')]
  if (htmlPlan) await applyAttributes(document.documentElement, htmlPlan.attributes, environment)
  if (headPlan) {
    const head = await renderNodes(headPlan.children || [], environment, 'head', null)
    document.head.replaceChildren(head, ...runtimeAssets)
  }
  const bodyNodes = bodyPlan ? bodyPlan.children || [] : (htmlPlan ? [] : plan.nodes || [])
  commitBody(await renderNodes(bodyNodes, environment, 'body', null), editingAtStart)
  for (const arguments_ of renderedMounts) mountComponent(...arguments_)
  for (const [key, record] of componentRecords) {
    if (renderedInstances.has(key)) continue
    cancelMount(key)
    disposeRuntime(record.owner)
    controllers.get(key)?.abort()
    controllers.delete(key)
    lifecycleStarted.delete(key)
    componentRecords.delete(key)
    instances.delete(key)
  }
}

const render = async () => {
  if (rendering) { renderAgain = true; return rendering }
  rendering = (async () => {
    do {
      renderAgain = false
      await nativeWrites
      const scroll = { x: scrollX, y: scrollY }
      await renderDocument(ownedEnvironment(pageOwner))
      scrollTo(scroll.x, scroll.y)
    } while (renderAgain)
    document.documentElement.dataset.tachyonRendered = 'client'
    delete document.documentElement.dataset.tachyonRenderError
  })().catch((error) => {
    document.documentElement.dataset.tachyonRenderError = 'render_failed'
    throw error
  }).finally(() => { rendering = null })
  return rendering
}

/// The methods `@onMount` named, recorded by the compiler on the class.
const mountMethods = (owner) => owner?.constructor?.__tachyonOnMount || []

// A retained publish/subscribe bus, scoped to the document.
//
// Retained because a component that mounts later still needs the value: the
// alternative is every subscriber racing every publisher, which is the bug
// this exists to avoid rather than a thing to leave to the application.
const tacSignalHandlers = new Map()
const tacRetained = new Map()
const ownerSubscriptions = new WeakMap()
let signalDepth = 0
let signalHandlerCount = 0

const signalName = (name) => {
  if (typeof name !== 'string' || !/^[A-Za-z0-9_$][A-Za-z0-9_.$-]{0,127}$/.test(name)) {
    throw new TypeError('Invalid Tac signal name.')
  }
  return name
}
const signalFailure = () => {
  document.documentElement.dataset.tachyonSignalError = 'subscriber_failed'
}
const deliverSignal = (handler, value) => {
  try { Promise.resolve(handler(value)).catch(signalFailure) } catch { signalFailure() }
}
const disposeRuntime = (owner) => {
  for (const unsubscribe of ownerSubscriptions.get(owner) || []) unsubscribe()
  ownerSubscriptions.delete(owner)
}

const tacPublish = (name, value, options = {}) => {
  signalName(name)
  if (signalDepth >= 32) throw new RangeError('Tac signal recursion exceeds 32 deliveries.')
  if (options.retain !== false) {
    const snapshot = structuredClone(value)
    const encoded = JSON.stringify(snapshot)
    if ((encoded?.length || 0) > 65536 || new TextEncoder().encode(encoded || '').byteLength > 65536) throw new RangeError('Tac retained value exceeds 64 KiB.')
    if (tacRetained.size >= 128 && !tacRetained.has(name)) tacRetained.delete(tacRetained.keys().next().value)
    tacRetained.set(name, snapshot)
  }
  signalDepth += 1
  try {
    for (const handler of [...(tacSignalHandlers.get(name) || [])]) deliverSignal(handler, value)
  } finally { signalDepth -= 1 }
}

const tacSubscribe = (name, handler, options = {}) => {
  signalName(name)
  if (typeof handler !== 'function') throw new TypeError('Tac subscriber must be a function.')
  if (options.signal?.aborted) return () => {}
  if (signalHandlerCount >= 1024 || (tacSignalHandlers.size >= 128 && !tacSignalHandlers.has(name))) {
    throw new RangeError('Tac subscription limit reached.')
  }
  const handlers = tacSignalHandlers.get(name) || new Set()
  const deliver = (value) => handler(value)
  handlers.add(deliver)
  signalHandlerCount += 1
  tacSignalHandlers.set(name, handlers)
  if (options.immediate && tacRetained.has(name)) deliverSignal(deliver, structuredClone(tacRetained.get(name)))
  const unsubscribe = () => {
    if (handlers.delete(deliver)) signalHandlerCount -= 1
    if (!handlers.size) tacSignalHandlers.delete(name)
    options.signal?.removeEventListener('abort', unsubscribe)
  }
  if (options.signal?.aborted) unsubscribe()
  else options.signal?.addEventListener('abort', unsubscribe, { once: true })
  return unsubscribe
}

// The storage surface a page or component companion reaches through `this.tac`.
const tacRuntime = Object.freeze({
  fetch: tacFetch,
  invalidate: invalidateCache,
  precache: precacheAssets,
  clearAssetCache: clearStaticCache,
  publish: tacPublish,
  subscribe: tacSubscribe,
  retained: (name) => structuredClone(tacRetained.get(signalName(name))),
  render,
})

// `tac` is deliberately not enumerable: a hot update snapshots owner state with
// structuredClone, which throws on a function and would drop the whole snapshot.
/// Applies what `@publish` and `@subscribe` declared.
///
/// The decorators are compiled away — no browser implements the proposal, so
/// shipping one would be shipping a syntax error — and what reaches here is
/// the list the compiler recorded instead. The behaviour is the decorator's
/// either way; only the delivery differs.
const bindSignals = (owner) => {
  for (const [member, signal, kind] of owner.constructor?.__tachyonPublish || []) {
    if (kind === 'method') {
      // A method publishes what it returns, and an async one what it resolves
      // to. A rejection publishes nothing: there is no value to publish.
      const original = owner[member]
      if (typeof original !== 'function') continue
      Object.defineProperty(owner, member, { configurable: true, value: function published(...args) {
        const result = original.apply(this, args)
        if (result && typeof result.then === 'function') {
          return result.then((detail) => {
            tacPublish(signal, detail)
            return detail
          })
        }
        tacPublish(signal, result)
        return result
      } })
      continue
    }
    // A field publishes what it holds, now and on every assignment.
    const descriptor = Object.getOwnPropertyDescriptor(owner, member)
    if (descriptor && !descriptor.configurable) continue
    let current = owner[member]
    Object.defineProperty(owner, member, {
      configurable: true,
      enumerable: true,
      get: () => descriptor?.get ? descriptor.get.call(owner) : current,
      set: (value) => {
        if (descriptor?.set) descriptor.set.call(owner, value)
        else current = value
        tacPublish(signal, value)
      },
    })
    tacPublish(signal, current)
  }

  for (const [member, signal, kind] of owner.constructor?.__tachyonSubscribe || []) {
    if (kind === 'method') {
      // Not called with the retained value: a handler firing for something
      // published before it existed is a surprise, where a field reading it is
      // the point of retaining it at all.
      owner.tac.subscribe(signal, async (value) => {
        if (typeof owner[member] === 'function') await owner[member](value)
        await render()
      })
      continue
    }
    owner.tac.subscribe(
      signal,
      (value) => {
        owner[member] = value
        queueMicrotask(() => { void render().catch(signalFailure) })
      },
      { immediate: true },
    )
  }
  return owner
}

const attachRuntime = (owner, scope) => {
  if (!owner || typeof owner !== 'object') return owner
  if (ownerSubscriptions.has(owner)) return owner
  const subscriptions = new Set()
  ownerSubscriptions.set(owner, subscriptions)
  const subscribe = (...args) => {
    const stop = tacSubscribe(...args)
    const unsubscribe = () => { stop(); subscriptions.delete(unsubscribe) }
    subscriptions.add(unsubscribe)
    return unsubscribe
  }
  Object.defineProperty(owner, 'tac', { configurable: true, value: Object.freeze({ ...tacRuntime, subscribe }) })
  return bindSignals(bindPersistentFields(owner, scope))
}

if (plan.module) {
  pageModule = await import(new URL(plan.module, location.href).href)
  if (typeof pageModule.default === 'function') pageOwner = new pageModule.default()
}
for (const [name, value] of Object.entries(plan.state || {})) {
  if (!(name in pageOwner)) pageOwner[name] = value
}

// Native method discovery never executes methods. Every call crosses the host
// bridge only when an authored event or expression explicitly requests it.
const nativeCall = async (request) => {
  const payload = JSON.stringify({ ...request, route: plan.route })
  if (payload.length > 65536) throw new RangeError('Tac native request exceeds 64 KiB.')
  let timer
  try {
    const raw = await Promise.race([
      Promise.resolve(globalThis.__tachyonNativeHostCall('companion.invoke', payload)),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Tac native call timed out.')), 10000) }),
    ])
    if (typeof raw === 'string' && raw.length > 65536) throw new RangeError('Tac native response exceeds 64 KiB.')
    const answer = typeof raw === 'string' ? JSON.parse(raw) : raw
    if (!answer || answer.error || answer.ok === false) throw new Error('Tac native call failed.')
    return answer.value
  } finally { clearTimeout(timer) }
}
const nativeMember = (name) => typeof name === 'string'
  && /^[A-Za-z_$][A-Za-z0-9_$]{0,127}$/.test(name)
  && !['__proto__', 'prototype', 'constructor', 'tac'].includes(name)

const bindNativeFields = (names) => {
  const state = new Map()
  const revisions = new Map()
  const pending = new Map()
  const readField = async (name, revision = revisions.get(name) ?? 0) => {
    const value = await nativeCall({ op: 'get', name })
    // A delayed acknowledgement must not replace a newer optimistic edit.
    if ((revisions.get(name) ?? 0) === revision) state.set(name, value)
  }
  const refresh = async () => {
    for (const name of names) {
      if (!pending.has(name)) await readField(name)
    }
  }
  for (const name of names) {
    Object.defineProperty(pageOwner, name, {
      configurable: true,
      enumerable: true,
      get: () => state.get(name),
      set: (value) => {
        if (nativeWriteCount >= 128) throw new RangeError('Tac native assignment queue exceeds 128 writes.')
        nativeWriteCount += 1
        const revision = (revisions.get(name) ?? 0) + 1
        revisions.set(name, revision)
        pending.set(name, revision)
        state.set(name, value)
        nativeWrites = nativeWrites.catch(() => {}).then(async () => {
          await nativeCall({ op: 'set', name, value })
          await readField(name, revision)
        }).finally(() => {
          nativeWriteCount -= 1
          if (pending.get(name) === revision) pending.delete(name)
        })
        void nativeWrites.catch(() => { document.documentElement.dataset.tachyonNativeError = 'assignment_failed' })
      },
    })
  }
  return refresh
}

if (typeof globalThis.__tachyonNativeHostCall === 'function') {
  const declared = await nativeCall({ op: 'init', props: {} })
  const fields = (Array.isArray(declared?.fields) ? declared.fields : []).slice(0, 128).filter(nativeMember)
  const refreshNativeFields = bindNativeFields(fields)
  await refreshNativeFields()
  for (const name of (Array.isArray(declared?.methods) ? declared.methods : []).slice(0, 128)) {
    if (nativeMember(name)) {
      const method = async (...args) => {
        await nativeWrites
        const answer = await nativeCall({ op: 'call', name, args })
        await refreshNativeFields()
        return answer
      }
      nativeMethods.add(method)
      pageOwner[name] = method
    }
  }
  const queued = Array.isArray(globalThis.__tachyonCompanionQueue) ? globalThis.__tachyonCompanionQueue.slice(-128) : []
  globalThis.__tachyonCompanionPublish = (signal) => {
    try { tacPublish(signal?.name, signal?.value) } catch { signalFailure() }
  }
  globalThis.__tachyonCompanionQueue = []
  for (const signal of queued) globalThis.__tachyonCompanionPublish(signal)
}
// Declared state is in place, so persisted fields can now override it. This
// runs before the mount hooks, which must observe the restored values.
attachRuntime(pageOwner, persistScope(plan.module || 'page'))
for (const method of mountMethods(pageOwner)) {
  if (typeof pageOwner[method] === 'function') await pageOwner[method]()
}
Object.defineProperty(globalThis, '__tachyonTac', {
  configurable: true,
  value: Object.freeze({ render, hotUpdate, instance: pageOwner, instances, tac: tacRuntime }),
})
globalThis.__tc_rerender = render
addEventListener('tachyon:rerender', render)
await render()

// Components mounted on visibility or interaction are still unfetched, so the
// offline cache is warmed once the page is idle rather than during the render
// that the user is waiting on.
const whenIdle = globalThis.requestIdleCallback || ((task) => setTimeout(task, 1))
whenIdle(() => {
  void precacheAssets(planAssets(plan.nodes).map((asset) => new URL(asset, location.href).href)).catch(() => {
    document.documentElement.dataset.tachyonCacheError = 'precache_failed'
  })
})
