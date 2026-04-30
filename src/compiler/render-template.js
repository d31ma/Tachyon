// @ts-check
/**
 * @typedef {Record<string, unknown>} TacProps
 * @typedef {Record<string, unknown> | null} TacController
 * @typedef {{ componentRootId: string | null, elemId: string | null, event: unknown }} RenderContext
 * @typedef {import('../runtime/tac.js').TacRuntimeBindings} TacRuntimeBindings
 */

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor
const TY_BOUND_PERSISTENT_FIELDS = '__ty_bound_persistent_fields__'
const TY_BOUND_REACTIVE_FIELDS = '__ty_bound_reactive_fields__'
const TY_INTERNAL_FIELDS = new Set(['props', 'tac', TY_BOUND_PERSISTENT_FIELDS, TY_BOUND_REACTIVE_FIELDS])

/**
 * @param {unknown} props
 * @returns {TacProps}
 */
const ty_decodeProps = (props) => {
    if (typeof props === 'string') {
        try {
            return JSON.parse(decodeURIComponent(props))
        } catch {
            return /** @type {TacProps} */ ({})
        }
    }

    return props && typeof props === 'object' ? /** @type {TacProps} */ (props) : /** @type {TacProps} */ ({})
}

/**
 * @param {TacController} controller
 * @param {unknown} props
 * @returns {Record<string, unknown>}
 */
const ty_createScope = (controller, props) => {
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
            if (key === '__ty_controller__') return controller
            if (key === '__ty_props__') return propBag
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

const __ty_isBrowserEnv = () => typeof window !== 'undefined'
    && !(/** @type {Record<string, unknown>} */ (globalThis).__ty_prerender__)

/**
 * @returns {Promise<IDBDatabase | null>}
 */
const __ty_openFetchCache = async () => {
    if (!__ty_isBrowserEnv() || typeof indexedDB === 'undefined')
        return null
    if (window.__ty_fetch_cache_db__)
        return window.__ty_fetch_cache_db__ ?? null
    window.__ty_fetch_cache_db__ = await new Promise((resolve) => {
        const request = indexedDB.open('tachyon-fetch-cache', 1)
        request.onupgradeneeded = () => {
            request.result.createObjectStore('responses', { keyPath: 'key' })
        }
        request.onsuccess = () => resolve(request.result)
        request.onerror = () => resolve(null)
    })
    return window.__ty_fetch_cache_db__ ?? null
}

/**
 * @param {string} cacheKey
 * @returns {Promise<Response | null>}
 */
const __ty_readCachedResponse = async (cacheKey) => {
    const db = await __ty_openFetchCache()
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
const __ty_writeCachedResponse = async (cacheKey, response) => {
    const db = await __ty_openFetchCache()
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
const __ty_deleteCachedResponse = async (cacheKey) => {
    const db = await __ty_openFetchCache()
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
    const browserEnv = __ty_isBrowserEnv()
    const canReadThroughCache = browserEnv
        && (method === 'GET' || method === 'HEAD')
        && request.cache !== 'no-store'
    const cacheKey = canReadThroughCache ? `${method}:${request.url}` : null
    if (cacheKey && request.cache !== 'reload') {
        const cached = await __ty_readCachedResponse(cacheKey)
        if (cached)
            return cached
    }
    try {
        const nativeFetch = /** @type {typeof fetch} */ (
            /** @type {Record<string, unknown>} */ (globalThis).__ty_native_fetch__ ?? fetch
        )
        const response = await nativeFetch(input, init)
        if (cacheKey && response.ok)
            void __ty_writeCachedResponse(cacheKey, response.clone())
        if (!cacheKey && response.ok && browserEnv) {
            void Promise.all([
                __ty_deleteCachedResponse(`GET:${request.url}`),
                __ty_deleteCachedResponse(`HEAD:${request.url}`),
            ])
        }
        return response
    } catch (error) {
        if (cacheKey) {
            const cached = await __ty_readCachedResponse(cacheKey)
            if (cached)
                return cached
        }
        throw error
    }
}

if (__ty_isBrowserEnv()) {
    const g = /** @type {Record<string, unknown>} */ (globalThis)
    if (!g.__ty_fetch_installed__) {
        g.__ty_fetch_installed__ = true
        g.__ty_native_fetch__ = globalThis.fetch.bind(globalThis)
        globalThis.fetch = /** @type {typeof fetch} */ (
            (input, init) => localFirstFetch(input, init)
        )
    }
}

/** @param {string} modulePath */
const ty_createHelpers = (modulePath) => {
    /** @type {RenderContext} */
    const renderContext = {
        componentRootId: null,
        elemId: null,
        event: undefined,
    }

    const isBrowser = typeof window !== 'undefined' && !globalThis.__ty_prerender__
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
            window.__ty_rerender?.()
        })
    }

    /** @param {() => void | Promise<void>} fn */
    const onMount = (fn) => {
        if (!isBrowser) return
        if (!window.__ty_onMount_queue__) window.__ty_onMount_queue__ = []
        window.__ty_onMount_queue__.push(fn)
    }

    const rerender = () => {
        if (isBrowser) window.__ty_rerender?.()
    }

    /**
     * @template T
     * @param {string} key
     * @param {T} [fallback]
     * @returns {T | undefined}
     */
    const inject = (key, fallback = undefined) => {
        if (!isBrowser) return fallback
        return /** @type {T | undefined} */ (window.__ty_context__?.get(key) ?? fallback)
    }

    /**
     * @template T
     * @param {string} key
     * @param {T} [fallback]
     * @returns {T | undefined}
     */
    const env = (key, fallback = undefined) => {
        if (!isBrowser) return fallback
        const publicEnv = /** @type {Record<string, unknown> | undefined} */ (window.__ty_public_env__)
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
            get: async () => ({ error: 'Fylo browser not enabled' }),
            patch: async () => ({ error: 'Fylo browser not enabled' }),
            del: async () => ({ error: 'Fylo browser not enabled' }),
        }
        const noopBase = {
            enabled: false,
            root: undefined,
            sql: async () => ({ error: 'Fylo browser not enabled' }),
            collections: async () => ({ root: '', collections: [] }),
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

    /**
     * @param {string} key
     * @param {unknown} value
     */
    const provide = (key, value) => {
        if (isBrowser) window.__ty_context__?.set(key, value)
    }

    /**
     * @param {Record<string, unknown>} props
     * @returns {string}
     */
    const resolvePersistScope = (props) => {
        const rawScope = props.__ty_persist_id__
            ?? (isBrowser ? window.location.pathname || '/' : modulePath || 'server')
        return `${modulePath || 'module'}:${String(rawScope)}`
    }

    /**
     * @param {string} name
     * @param {unknown} detail
     * @returns {boolean}
     */
    const emit = (name, detail) => {
        const eventName = String(name || '').replace(/^@/, '')
        const targetId = renderContext.componentRootId

        if (!eventName || !targetId || typeof document === 'undefined') return false

        const target = document.getElementById(targetId)
        if (!target || typeof CustomEvent === 'undefined') return false

        return target.dispatchEvent(new CustomEvent(eventName, {
            detail,
            bubbles: true,
            composed: true,
        }))
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
     * @param {Record<string, unknown>} controller
     * @param {TacProps} props
     */
    const bindPersistentFields = (controller, props) => {
        const boundFields = controller[TY_BOUND_PERSISTENT_FIELDS] instanceof Set
            ? controller[TY_BOUND_PERSISTENT_FIELDS]
            : new Set()
        controller[TY_BOUND_PERSISTENT_FIELDS] = boundFields
        const persistScope = resolvePersistScope(props)
        for (const fieldName of Object.keys(controller)) {
            if (!fieldName.startsWith('$') || boundFields.has(fieldName))
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
     */
    const bindReactiveFields = (controller) => {
        const boundFields = controller[TY_BOUND_REACTIVE_FIELDS] instanceof Set
            ? controller[TY_BOUND_REACTIVE_FIELDS]
            : new Set()
        controller[TY_BOUND_REACTIVE_FIELDS] = boundFields

        for (const fieldName of Object.keys(controller)) {
            if (TY_INTERNAL_FIELDS.has(fieldName) || boundFields.has(fieldName))
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
                        if (!Object.is(previousValue, currentValue))
                            scheduleRerender()
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
        /** @param {Record<string, unknown>} controller */
        bindPersistentFields(controller) {
            bindPersistentFields(controller, props)
        },
        env,
        props,
        emit,
        /** @param {RequestInfo | URL} input @param {RequestInit} [init] */
        fetch: (input, init) => localFirstFetch(input, init),
        inject,
        onMount,
        provide,
        rerender,
    })

    /**
     * Post-construction binding for a companion instance: assigns props/tac
     * (defensive — Tac's constructor already does this for `extends Tac`),
     * merges props onto same-named instance fields, and then binds persistent
     * ($-prefixed) fields. Props must run before persistence so they act as
     * defaults; stored session values still win after a reload.
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
        }
        finally {
            suppressReactiveRerender = false
        }
    }

    /**
     * @param {string} modulePath
     * @returns {Promise<Function>}
     */
    const loadTacModule = async (modulePath) => {
        const tacGlobal = typeof window !== 'undefined' ? window.Tac : undefined
        if (tacGlobal?.load) return /** @type {Promise<Function>} */ (tacGlobal.load(modulePath))

        const resolved = new URL(import.meta.url)
        resolved.pathname = resolved.pathname.replace(/\/(?:pages|components)\/.*$/, modulePath)

        const mod = await import(resolved.href)
        if (typeof mod.default === 'function') return mod.default

        throw new Error(`Tac module "${modulePath}" did not export a renderer`)
    }

    return {
        createTacHelpers: tacHelpers,
        bindCompanion,
        createScope: ty_createScope,
        decodeProps: ty_decodeProps,
        env,
        emit,
        inject,
        isBrowser,
        isServer,
        onMount,
        provide,
        rerender,
        fylo,
        loadTacModule,
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

const __ty_module_imports__ = {
// module_imports
}

const __ty_compiled_factory__ = new AsyncFunction('__ty_helpers__', '__ty_module_imports__', 'props', "__TY_FACTORY_SOURCE__")

/** @param {unknown} props */
export default async function (props) {
    return await __ty_compiled_factory__(ty_createHelpers("__TY_MODULE_PATH__"), __ty_module_imports__, props)
}
