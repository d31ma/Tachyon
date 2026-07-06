// @ts-check
/**
 * @typedef {Record<string, unknown>} TacProps
 * @typedef {Record<string, unknown> | null} TacController
 * @typedef {{ componentRootId: string | null, elemId: string | null, event: unknown }} RenderContext
 * @typedef {import('../runtime/tac.js').TacRuntimeBindings} TacRuntimeBindings
 */

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor
const TC_BOUND_PERSISTENT_FIELDS = '__tc_bound_persistent_fields__'
const TC_BOUND_REACTIVE_FIELDS = '__tc_bound_reactive_fields__'
const TC_SIGNAL_PUBLISH_FIELDS = '__tc_signal_publish_fields__'
const TC_INTERNAL_FIELDS = new Set(['props', 'tac', TC_BOUND_PERSISTENT_FIELDS, TC_BOUND_REACTIVE_FIELDS, TC_SIGNAL_PUBLISH_FIELDS])

/** @param {string} name */
const tc_camelCasePropName = (name) => name.replace(/-([a-zA-Z0-9])/g, (_match, char) => char.toUpperCase())

/**
 * @param {unknown} props
 * @returns {TacProps}
 */
const tc_decodeProps = (props) => {
    /** @param {TacProps} propBag */
    const withCamelAliases = (propBag) => {
        for (const key of Object.keys(propBag)) {
            if (!key.includes('-')) continue
            const camelKey = tc_camelCasePropName(key)
            if (camelKey !== key && !Object.prototype.hasOwnProperty.call(propBag, camelKey))
                propBag[camelKey] = propBag[key]
        }
        return propBag
    }

    if (typeof props === 'string') {
        try {
            return withCamelAliases(JSON.parse(decodeURIComponent(props)))
        } catch {
            return /** @type {TacProps} */ ({})
        }
    }

    return props && typeof props === 'object' ? withCamelAliases(/** @type {TacProps} */ (props)) : /** @type {TacProps} */ ({})
}

/**
 * @param {TacController} controller
 * @param {unknown} props
 * @returns {Record<string, unknown>}
 */
const tc_createScope = (controller, props) => {
    /** @type {Record<string, unknown>} */
    const state = Object.create(null)
    /** @type {TacProps} */
    const propBag = props && typeof props === 'object' ? /** @type {TacProps} */ (props) : /** @type {TacProps} */ ({})

    /** @type {Record<string, unknown>} */
    let proxy
    proxy = new Proxy(state, {
        has(_target, key) {
            if (key === Symbol.unscopables || typeof key !== 'string') return false
            return Object.prototype.hasOwnProperty.call(state, key)
                || (controller ? key in controller : false)
                || key in propBag
        },
        get(_target, key) {
            if (key === Symbol.unscopables) return undefined
            if (key === '__tc_controller__') return controller
            if (key === '__tc_props__') return propBag
            if (typeof key !== 'string') return undefined

            if (Object.prototype.hasOwnProperty.call(state, key)) return state[key]

            if (controller && key in controller) {
                const value = controller[key]
                return typeof value === 'function' ? value.bind(controller) : value
            }

            return propBag[key]
        },
        set(_target, key, value) {
            if (typeof key !== 'string') return true

            if (controller && key in controller) {
                controller[key] = value
                return true
            }

            if (key in propBag) {
                propBag[key] = value
                return true
            }

            state[key] = value
            return true
        },
        ownKeys() {
            const keys = new Set(Object.keys(state))
            if (controller && typeof controller === 'object') {
                for (const key of Object.keys(controller)) keys.add(key)
            }
            for (const key of Object.keys(propBag)) keys.add(key)
            return [...keys]
        },
        /**
         * @param {Record<string, unknown>} _target
         * @param {string | symbol} key
         * @returns {PropertyDescriptor | undefined}
         */
        getOwnPropertyDescriptor(_target, key) {
            if (typeof key !== 'string') return undefined
            return {
                configurable: true,
                enumerable: true,
                writable: true,
                value: proxy[key],
            }
        },
    })

    return proxy
}

const __tc_isBrowserEnv = () => typeof window !== 'undefined'
    && !(/** @type {Record<string, unknown>} */ (globalThis).__tc_prerender__)

const __tc_defaultPlatform = Object.freeze({
    target: 'web',
    platform: 'web',
    environment: 'browser',
    os: 'unknown',
    browserOS: 'unknown',
    native: false,
    browser: true,
    web: true,
    desktop: false,
    mobile: false,
})

const __tc_platformContext = () => {
    const tacGlobal = /** @type {{ Tac?: { platform?: unknown } }} */ (globalThis).Tac
    const candidate = tacGlobal?.platform
    return candidate && typeof candidate === 'object'
        ? /** @type {typeof __tc_defaultPlatform} */ (candidate)
        : __tc_defaultPlatform
}

/**
 * @returns {Promise<IDBDatabase | null>}
 */
const __tc_openFetchCache = async () => {
    if (!__tc_isBrowserEnv() || typeof indexedDB === 'undefined')
        return null
    if (window.__tc_fetch_cache_db__)
        return window.__tc_fetch_cache_db__ ?? null
    window.__tc_fetch_cache_db__ = await new Promise((resolve) => {
        const request = indexedDB.open('tachyon-fetch-cache', 1)
        request.onupgradeneeded = () => {
            request.result.createObjectStore('responses', { keyPath: 'key' })
        }
        request.onsuccess = () => resolve(request.result)
        request.onerror = () => resolve(null)
    })
    return window.__tc_fetch_cache_db__ ?? null
}

/**
 * @param {string} cacheKey
 * @returns {Promise<Response | null>}
 */
const __tc_readCachedResponse = async (cacheKey) => {
    const db = await __tc_openFetchCache()
    if (!db)
        return null
    return await new Promise((resolve) => {
        const tx = db.transaction('responses', 'readonly')
        const request = tx.objectStore('responses').get(cacheKey)
        request.onsuccess = () => {
            const entry = request.result
            if (!entry) {
                resolve(null)
                return
            }
            resolve(new Response(entry.body ? new Uint8Array(entry.body) : null, {
                status: entry.status,
                statusText: entry.statusText,
                headers: entry.headers,
            }))
        }
        request.onerror = () => resolve(null)
    })
}

/**
 * @param {string} cacheKey
 * @param {Response} response
 * @returns {Promise<void>}
 */
const __tc_writeCachedResponse = async (cacheKey, response) => {
    const db = await __tc_openFetchCache()
    if (!db)
        return
    const body = await response.arrayBuffer()
    await new Promise((resolve) => {
        const tx = db.transaction('responses', 'readwrite')
        tx.oncomplete = () => resolve(undefined)
        tx.onerror = () => resolve(undefined)
        tx.objectStore('responses').put({
            key: cacheKey,
            status: response.status,
            statusText: response.statusText,
            headers: Object.fromEntries(response.headers.entries()),
            body,
            updatedAt: Date.now(),
        })
    })
}

/**
 * @param {string} cacheKey
 * @returns {Promise<void>}
 */
const __tc_deleteCachedResponse = async (cacheKey) => {
    const db = await __tc_openFetchCache()
    if (!db)
        return
    await new Promise((resolve) => {
        const tx = db.transaction('responses', 'readwrite')
        tx.oncomplete = () => resolve(undefined)
        tx.onerror = () => resolve(undefined)
        tx.objectStore('responses').delete(cacheKey)
    })
}

/**
 * @param {RequestInfo | URL} input
 * @param {RequestInit} [init]
 * @returns {Promise<Response>}
 */
const localFirstFetch = async (input, init) => {
    const request = new Request(input, init)
    const method = request.method.toUpperCase()
    const browserEnv = __tc_isBrowserEnv()
    const sharedCache = /** @type {{ fetch?: Function } | undefined} */ (
        /** @type {Record<string, unknown>} */ (globalThis).__tc_browser_cache__
    )
    if (browserEnv && typeof sharedCache?.fetch === 'function') {
        const canCacheRead = (method === 'GET' || method === 'HEAD') && request.cache !== 'no-store'
        return await sharedCache.fetch(input, init, {
            key: canCacheRead ? `${method}:${request.url}` : null,
            invalidateKeys: method === 'GET' || method === 'HEAD'
                ? []
                : [`GET:${request.url}`, `HEAD:${request.url}`],
        })
    }
    const canReadThroughCache = browserEnv
        && (method === 'GET' || method === 'HEAD')
        && request.cache !== 'no-store'
    const cacheKey = canReadThroughCache ? `${method}:${request.url}` : null
    if (cacheKey && request.cache !== 'reload') {
        const cached = await __tc_readCachedResponse(cacheKey)
        if (cached)
            return cached
    }
    try {
        const nativeFetch = /** @type {typeof fetch} */ (
            /** @type {Record<string, unknown>} */ (globalThis).__tc_native_fetch__ ?? fetch
        )
        const response = await nativeFetch(input, init)
        if (cacheKey && response.ok)
            void __tc_writeCachedResponse(cacheKey, response.clone())
        if (!cacheKey && response.ok && browserEnv) {
            void Promise.all([
                __tc_deleteCachedResponse(`GET:${request.url}`),
                __tc_deleteCachedResponse(`HEAD:${request.url}`),
            ])
        }
        return response
    } catch (error) {
        if (cacheKey) {
            const cached = await __tc_readCachedResponse(cacheKey)
            if (cached)
                return cached
        }
        throw error
    }
}

if (__tc_isBrowserEnv()) {
    const g = /** @type {Record<string, unknown>} */ (globalThis)
    if (!g.__tc_fetch_installed__) {
        g.__tc_fetch_installed__ = true
        g.__tc_native_fetch__ = globalThis.fetch.bind(globalThis)
        globalThis.fetch = /** @type {typeof fetch} */ (
            (input, init) => localFirstFetch(input, init)
        )
    }
}

/** @param {string} modulePath */
const tc_createHelpers = (modulePath) => {
    /** @type {RenderContext} */
    const renderContext = {
        componentRootId: null,
        elemId: null,
        event: undefined,
    }

    const isBrowser = typeof window !== 'undefined' && !globalThis.__tc_prerender__
    const isServer = !isBrowser
    let rerenderScheduled = false
    let suppressReactiveRerender = false

    const scheduleRerender = () => {
        if (!isBrowser || suppressReactiveRerender || renderContext.elemId)
            return
        if (rerenderScheduled)
            return
        rerenderScheduled = true
        queueMicrotask(() => {
            rerenderScheduled = false
            // Scope the reactive refresh to this instance's own component subtree;
            // the page root (no componentRootId) falls back to a full refresh.
            window.__tc_rerender?.(renderContext.componentRootId ?? undefined)
        })
    }

    /** @param {() => void | Promise<void>} fn */
    const onMount = (fn) => {
        if (!isBrowser) return
        if (!window.__tc_onMount_queue__) window.__tc_onMount_queue__ = []
        window.__tc_onMount_queue__.push(fn)
    }

    /**
     * Triggers a re-render. Scoped to the calling component by default; pass
     * `{ global: true }` to force a full-page refresh (escape hatch for the rare
     * case a component must update content rendered by its parent/page —
     * prefer publish/subscribe for cross-component updates).
     * @param {{ global?: boolean }} [options]
     */
    const rerender = (options) => {
        if (!isBrowser) return
        const hostId = options && options.global ? undefined : (renderContext.componentRootId ?? undefined)
        window.__tc_rerender?.(hostId)
    }

    /**
     * @template T
     * @param {string} key
     * @param {T} [fallback]
     * @returns {T | undefined}
     */
    const env = (key, fallback = undefined) => {
        if (!isBrowser) return fallback
        const publicEnv = /** @type {Record<string, unknown> | undefined} */ (window.__tc_public_env__)
        if (!publicEnv || !(key in publicEnv)) return fallback
        return /** @type {T | undefined} */ (publicEnv[key])
    }

    /**
     * FYLO global available inside rendered template expressions (e.g. inline
     * `{ fylo.users.find() }` interpolations). Companion scripts get a separate
     * `import { fylo }` injected by the compiler — see Compiler.referencesFyloGlobal.
     * Both delegate to window.fylo at access time, falling back to a noop Proxy
     * during prerender or when /_fylo isn't mounted.
     */
    const fylo = (() => {
        const noopCollection = {
            find: async () => ({ error: 'Fylo browser not enabled' }),
            list: async () => ({ error: 'Fylo browser not enabled' }),
            get: async () => ({ error: 'Fylo browser not enabled' }),
            events: async () => ({ error: 'Fylo browser not enabled' }),
            patch: async () => ({ error: 'Fylo browser not enabled' }),
            del: async () => ({ error: 'Fylo browser not enabled' }),
            rebuild: async () => ({ error: 'Fylo browser not enabled' }),
        }
        const noopBase = {
            enabled: false,
            root: undefined,
            setCredentials() {},
            clearCredentials() {},
            sql: async () => ({ error: 'Fylo browser not enabled' }),
            collections: async () => ({ root: '', collections: [] }),
            meta: async () => null,
        }
        const noop = new Proxy(noopBase, {
            get(target, prop) {
                if (typeof prop === 'string' && !(prop in target)) return noopCollection
                return Reflect.get(target, prop)
            },
        })
        return new Proxy(noop, {
            get(_, prop) {
                const live = (typeof window !== 'undefined' ? /** @type {any} */ (window).fylo : undefined) ?? noop
                return Reflect.get(live, prop)
            },
        })
    })()

    /** @param {string} name */
    const signalName = (name) => String(name || '').trim()

    const signalHub = () => {
        if (!isBrowser) return null
        const targetWindow = /** @type {Window & { __tc_signals__?: { values: Map<string, unknown>, listeners: Map<string, Set<(value: unknown) => void | Promise<void>>> } }} */ (window)
        if (!targetWindow.__tc_signals__) {
            targetWindow.__tc_signals__ = {
                values: new Map(),
                listeners: new Map(),
            }
        }
        return targetWindow.__tc_signals__
    }

    /**
     * @param {(value: unknown) => void | Promise<void>} listener
     * @param {unknown} value
     */
    const notifySignalListener = (listener, value) => {
        try {
            const result = listener(value)
            if (result && typeof result.then === 'function') {
                result.catch(() => {})
            }
        } catch {
            // Signal subscribers are isolated so one listener cannot block
            // delivery to the rest of the page/component tree.
        }
    }

    /**
     * @param {Record<string, unknown>} props
     * @returns {string}
     */
    const resolvePersistScope = (props) => {
        const rawScope = props.__tc_persist_id__
            ?? (isBrowser ? window.location.pathname || '/' : modulePath || 'server')
        return `${modulePath || 'module'}:${String(rawScope)}`
    }

    /**
     * @param {string} name
     * @param {unknown} value
     * @param {{ retain?: boolean }} [options]
     * @returns {boolean}
     */
    const publish = (name, value, options = {}) => {
        const resolvedName = signalName(name)
        const hub = signalHub()
        if (!resolvedName || !hub) return false
        if (options.retain) hub.values.set(resolvedName, value)
        const listeners = hub.listeners.get(resolvedName)
        if (!listeners || listeners.size === 0) return false
        for (const listener of [...listeners]) {
            notifySignalListener(listener, value)
        }
        return true
    }

    /**
     * @template T
     * @param {string} name
     * @param {((value: unknown) => void | Promise<void>) | T} [callbackOrFallback]
     * @param {{ immediate?: boolean }} [options]
     * @returns {(() => void) | T | undefined}
     */
    const subscribe = (name, callbackOrFallback = undefined, options = {}) => {
        const resolvedName = signalName(name)
        const hub = signalHub()
        if (!resolvedName || !hub) {
            return typeof callbackOrFallback === 'function' ? () => {} : callbackOrFallback
        }
        if (typeof callbackOrFallback !== 'function') {
            return /** @type {T | undefined} */ (hub.values.has(resolvedName)
                ? hub.values.get(resolvedName)
                : callbackOrFallback)
        }
        const listener = /** @type {(value: unknown) => void | Promise<void>} */ (callbackOrFallback)
        let listeners = hub.listeners.get(resolvedName)
        if (!listeners) {
            listeners = new Set()
            hub.listeners.set(resolvedName, listeners)
        }
        listeners.add(listener)
        if (options.immediate !== false && hub.values.has(resolvedName)) {
            notifySignalListener(listener, hub.values.get(resolvedName))
        }
        return () => {
            listeners?.delete(listener)
            if (listeners && listeners.size === 0) hub.listeners.delete(resolvedName)
        }
    }

    /**
     * @param {string} storageKey
     * @param {unknown} fallback
     * @returns {unknown}
     */
    const readSessionValue = (storageKey, fallback = undefined) => {
        if (!isBrowser) return fallback
        try {
            const stored = sessionStorage.getItem(storageKey)
            return stored === null ? fallback : JSON.parse(stored)
        } catch {
            return fallback
        }
    }

    /**
     * @param {string} storageKey
     * @param {unknown} value
     */
    const writeSessionValue = (storageKey, value) => {
        if (!isBrowser) return
        try {
            if (value === undefined) {
                sessionStorage.removeItem(storageKey)
                return
            }
            sessionStorage.setItem(storageKey, JSON.stringify(value))
        } catch {}
    }

    /**
     * @param {string} storageKey
     * @param {unknown} fallback
     * @returns {unknown}
     */
    const readLocalValue = (storageKey, fallback = undefined) => {
        if (!isBrowser) return fallback
        try {
            const stored = globalThis.localStorage.getItem(storageKey)
            return stored === null ? fallback : JSON.parse(stored)
        } catch {
            return fallback
        }
    }

    /**
     * @param {string} storageKey
     * @param {unknown} value
     */
    const writeLocalValue = (storageKey, value) => {
        if (!isBrowser) return
        try {
            if (value === undefined) {
                globalThis.localStorage.removeItem(storageKey)
                return
            }
            globalThis.localStorage.setItem(storageKey, JSON.stringify(value))
        } catch {}
    }

    /**
     * @param {Record<string, unknown>} controller
     * @param {TacProps} props
     */
    const bindPersistentFields = (controller, props) => {
        const boundFields = controller[TC_BOUND_PERSISTENT_FIELDS] instanceof Set
            ? controller[TC_BOUND_PERSISTENT_FIELDS]
            : new Set()
        controller[TC_BOUND_PERSISTENT_FIELDS] = boundFields
        const persistScope = resolvePersistScope(props)
        for (const fieldName of Object.keys(controller)) {
            if (boundFields.has(fieldName))
                continue
            if (fieldName.startsWith('$$')) {
                const storageKey = `tac:${persistScope}:${fieldName}`
                let currentValue = readLocalValue(storageKey, controller[fieldName])
                Object.defineProperty(controller, fieldName, {
                    configurable: true,
                    enumerable: true,
                    get() {
                        return currentValue
                    },
                    set(nextValue) {
                        currentValue = nextValue
                        writeLocalValue(storageKey, nextValue)
                    },
                })
                controller[fieldName] = currentValue
                boundFields.add(fieldName)
                continue
            }
            if (!fieldName.startsWith('$'))
                continue
            const storageKey = `tac:${persistScope}:${fieldName}`
            let currentValue = readSessionValue(storageKey, controller[fieldName])
            Object.defineProperty(controller, fieldName, {
                configurable: true,
                enumerable: true,
                get() {
                    return currentValue
                },
                set(nextValue) {
                    currentValue = nextValue
                    writeSessionValue(storageKey, nextValue)
                },
            })
            controller[fieldName] = currentValue
            boundFields.add(fieldName)
        }
    }

    /**
     * @param {Record<string, unknown>} controller
     * @param {string} fieldName
     * @param {unknown} value
     */
    const publishSignalField = (controller, fieldName, value) => {
        const fields = Array.isArray(controller[TC_SIGNAL_PUBLISH_FIELDS])
            ? controller[TC_SIGNAL_PUBLISH_FIELDS]
            : []
        for (const field of fields) {
            if (field && field.field === fieldName) {
                publish(String(field.name || ''), value, field.options || { retain: true })
            }
        }
    }

    /**
     * @param {Record<string, unknown>} controller
     */
    const publishSignalFields = (controller) => {
        const fields = Array.isArray(controller[TC_SIGNAL_PUBLISH_FIELDS])
            ? controller[TC_SIGNAL_PUBLISH_FIELDS]
            : []
        for (const field of fields) {
            if (field && typeof field.field === 'string' && field.field in controller) {
                publish(String(field.name || ''), controller[field.field], field.options || { retain: true })
            }
        }
    }

    /**
     * @param {Record<string, unknown>} controller
     */
    const bindReactiveFields = (controller) => {
        const boundFields = controller[TC_BOUND_REACTIVE_FIELDS] instanceof Set
            ? controller[TC_BOUND_REACTIVE_FIELDS]
            : new Set()
        controller[TC_BOUND_REACTIVE_FIELDS] = boundFields

        for (const fieldName of Object.keys(controller)) {
            if (TC_INTERNAL_FIELDS.has(fieldName) || boundFields.has(fieldName))
                continue
            const descriptor = Object.getOwnPropertyDescriptor(controller, fieldName)
            if (!descriptor || descriptor.configurable === false)
                continue

            if ('value' in descriptor) {
                let currentValue = descriptor.value
                Object.defineProperty(controller, fieldName, {
                    configurable: true,
                    enumerable: descriptor.enumerable,
                    get() {
                        return currentValue
                    },
                    set(nextValue) {
                        if (Object.is(currentValue, nextValue))
                            return
                        currentValue = nextValue
                        publishSignalField(controller, fieldName, nextValue)
                        scheduleRerender()
                    },
                })
                boundFields.add(fieldName)
                continue
            }

            if (typeof descriptor.get === 'function' && typeof descriptor.set === 'function') {
                Object.defineProperty(controller, fieldName, {
                    configurable: true,
                    enumerable: descriptor.enumerable,
                    get() {
                        return descriptor.get?.call(controller)
                    },
                    set(nextValue) {
                        const previousValue = descriptor.get?.call(controller)
                        descriptor.set?.call(controller, nextValue)
                        const currentValue = descriptor.get?.call(controller)
                        if (!Object.is(previousValue, currentValue)) {
                            publishSignalField(controller, fieldName, currentValue)
                            scheduleRerender()
                        }
                    },
                })
                boundFields.add(fieldName)
            }
        }
    }

    /** @param {TacProps} props */
    const tacHelpers = (props) => ({
        get isBrowser() { return isBrowser },
        get isServer() { return isServer },
        get platform() { return __tc_platformContext() },
        /** @param {Record<string, unknown>} controller */
        bindPersistentFields(controller) {
            bindPersistentFields(controller, props)
        },
        env,
        props,
        /** @param {RequestInfo | URL} input @param {RequestInit} [init] */
        fetch: (input, init) => localFirstFetch(input, init),
        onMount,
        publish,
        rerender,
        subscribe,
    })

    /**
     * Post-construction binding for a companion instance: assigns props/tac
     * (defensive — Tac's constructor already does this for `extends Tac`),
     * merges props onto same-named instance fields, and then binds persistent
     * ($-prefixed for sessionStorage, $$-prefixed for localStorage) fields.
     * Props must run before persistence so they act as defaults; stored
     * session / local values still win after a reload.
     *
     * @param {Record<string, unknown>} instance
     * @param {TacProps} props
     * @param {TacRuntimeBindings} tac
     */
    const bindCompanion = (instance, props, tac) => {
        instance.props = props
        instance.tac = tac
        suppressReactiveRerender = true
        try {
            if (props) {
                const propBag = /** @type {Record<string, unknown>} */ (props)
                for (const fieldName of Object.keys(instance)) {
                    if (fieldName === 'props' || fieldName === 'tac') continue
                    if (Object.prototype.hasOwnProperty.call(propBag, fieldName)) {
                        instance[fieldName] = propBag[fieldName]
                        continue
                    }
                    if (fieldName.startsWith('$$')) {
                        const stripped = fieldName.slice(2)
                        if (Object.prototype.hasOwnProperty.call(propBag, stripped)) {
                            instance[fieldName] = propBag[stripped]
                        }
                        continue
                    }
                    if (fieldName.startsWith('$')) {
                        const stripped = fieldName.slice(1)
                        if (Object.prototype.hasOwnProperty.call(propBag, stripped)) {
                            instance[fieldName] = propBag[stripped]
                        }
                    }
                }
            }
            bindPersistentFields(instance, props)
            bindReactiveFields(instance)
            publishSignalFields(instance)
        }
        finally {
            suppressReactiveRerender = false
        }
    }

    /**
     * @param {string} targetPath
     * @returns {string}
     */
    const toRelativeModulePath = (targetPath) => {
        const fromParts = modulePath.split('/').filter(Boolean)
        const toParts = targetPath.split('/').filter(Boolean)
        fromParts.pop()
        while (fromParts.length && toParts.length && fromParts[0] === toParts[0]) {
            fromParts.shift()
            toParts.shift()
        }
        const relativeParts = [...fromParts.map(() => '..'), ...toParts]
        const relative = relativeParts.join('/') || '.'
        return relative.startsWith('.') ? relative : `./${relative}`
    }
    /**
     * @param {string} targetPath
     * @returns {Promise<(...args: any[]) => any>}
     */
    const loadTacModule = async (targetPath) => {
        const tacGlobal = typeof window !== 'undefined' ? window.Tac : undefined
        // Absolute paths are root-relative URLs; the Tac registry knows how to
        // resolve them for both browser runtime and server-side prerender.
        if (targetPath.startsWith('/') && tacGlobal?.load) {
            return /** @type {Promise<(...args: any[]) => any>} */ (tacGlobal.load(targetPath))
        }

        const importPath = targetPath.startsWith('/') ? toRelativeModulePath(targetPath) : targetPath
        const resolved = new URL(importPath, import.meta.url)
        const module = await import(resolved.href)
        const factory = /** @type {(...args: any[]) => any} */ (module.default)
        if (typeof factory === 'function') {
            tacGlobal?.register?.(targetPath, factory)
            return factory
        }

        throw new Error(`Tac module "${targetPath}" did not export a renderer`)
    }

    return {
        createTacHelpers: tacHelpers,
        bindCompanion,
        createScope: tc_createScope,
        decodeProps: tc_decodeProps,
        env,
        isBrowser,
        isServer,
        get platform() { return __tc_platformContext() },
        onMount,
        publish,
        rerender,
        subscribe,
        fylo,
        loadTacModule,
        /**
         * Registers the event types this module uses for document-level delegation.
         * The compiler injects one call per module (the compile-time event set), so
         * the runtime never scans the DOM to discover handlers. No-op during
         * server prerender (no Tac runtime / document).
         * @param {string[]} eventNames
         */
        delegateEvents(eventNames) {
            const runtime = /** @type {{ delegateEvents?: (names: string[]) => void }} */ (
                /** @type {any} */ (globalThis).Tac);
            if (runtime && typeof runtime.delegateEvents === 'function')
                runtime.delegateEvents(eventNames);
        },
        /**
         * Registers a component's render closure by host id so the runtime can
         * re-render just this component's subtree. The compiler injects one call
         * per component instance. No-op during server prerender.
         * @param {string} hostId
         * @param {(elemId?: string | null, event?: unknown, compId?: string | null) => Promise<string>} render
         * @param {string} compId
         */
        registerComponentRender(hostId, render, compId) {
            const runtime = /** @type {{ registerComponentRender?: (h: string, r: unknown, c: string) => void }} */ (
                /** @type {any} */ (globalThis).Tac);
            if (runtime && typeof runtime.registerComponentRender === 'function')
                runtime.registerComponentRender(hostId, render, compId);
        },
        /**
         * @param {unknown} switchValue
         * @param {unknown} caseValue
         */
        matchSwitchCase(switchValue, caseValue) {
            return Array.isArray(caseValue)
                ? caseValue.some((value) => Object.is(value, switchValue))
                : Object.is(caseValue, switchValue)
        },
        /**
         * @param {{ componentRootId?: string | null, elemId?: string | null, event?: unknown }} context
         */
        setRenderContext(context) {
            renderContext.componentRootId = context.componentRootId ?? null
            renderContext.elemId = context.elemId ?? null
            renderContext.event = context.event
        },
    }
}

const __tc_module_imports__ = {
// module_imports
}

const __tc_compiled_factory__ = new AsyncFunction('__tc_helpers__', '__tc_module_imports__', 'props', "__TY_FACTORY_SOURCE__")

/** @param {unknown} props */
export default async function (props) {
    return await __tc_compiled_factory__(tc_createHelpers("__TY_MODULE_PATH__"), __tc_module_imports__, props)
}
