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
    environment: 'web',
    os: 'web',
    browserOS: 'unknown',
    native: false,
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
    const isNativeController = !isBrowser
        && !globalThis.__tc_prerender__
        && typeof (/** @type {any} */ (globalThis).__tcNativeBridge__)?.invoke === 'function'
        && typeof (/** @type {any} */ (globalThis).__tcNativeBridge__)?.onMessage === 'function'
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
        if (!isBrowser) {
            // The DOM-free controller installs its bridge before it creates a
            // route factory. Run mount work on the next microtask so companion
            // construction and binding finish first, while server prerendering
            // remains a no-op.
            if (!isNativeController) return
            queueMicrotask(() => {
                try {
                    const result = fn()
                    if (result instanceof Promise)
                        result.catch((error) => console.error('[tachyon] onMount callback error:', error))
                }
                catch (error) {
                    console.error('[tachyon] onMount callback error:', error)
                }
            })
            return
        }
        // A controller can register after the renderer already flushed the
        // mount queue — e.g. a Dart companion whose runtime module loads
        // asynchronously. Late registrations run on the next microtask,
        // mirroring how a load listener fires immediately on a loaded page.
        if (window.__tc_onMount_flushed__) {
            queueMicrotask(() => {
                try {
                    const result = fn()
                    if (result instanceof Promise)
                        result.catch((error) => console.error('[tachyon] onMount callback error:', error))
                }
                catch (error) {
                    console.error('[tachyon] onMount callback error:', error)
                }
            })
            return
        }
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

    /** @param {string} capability */
    const isDeviceCapabilityAuthorized = (capability) => {
        const bridge = /** @type {{ supports?: (capability: string) => boolean } | undefined} */ (
            /** @type {any} */ (globalThis).__tcNativeBridge__ ?? (isBrowser ? /** @type {any} */ (window).__tcNativeBridge__ : undefined))
        if (typeof bridge?.supports === 'function' && bridge.supports(capability)) return true
        if (!isBrowser) return false
        const meta = [...window.document.getElementsByTagName('meta')]
            .find((element) => element.getAttribute('name') === 'tachyon-native-capabilities')
        const declared = meta?.getAttribute('content') ?? ''
        return declared.split(',').map((entry) => entry.trim()).filter(Boolean).some((entry) => (
            entry === capability || (entry.endsWith('.*') && capability.startsWith(entry.slice(0, -1)))
        ))
    }

    // Lets portable companions describe an unavailable capability without
    // attempting an invocation that would fail closed at the device boundary.
    /** @param {string} capability */
    const deviceAvailable = (capability) => isDeviceCapabilityAuthorized(capability)

    /** @param {string} capability */
    const standardCapabilityAvailable = (capability) => {
        if (!isBrowser) return false
        switch (capability) {
            case 'geo.current': return nativePermissionDeclared('location') && typeof window.navigator?.geolocation?.getCurrentPosition === 'function'
            case 'notify.show': return nativePermissionDeclared('notifications') && typeof window.Notification === 'function'
            case 'media.getUserMedia':
                return (nativePermissionDeclared('camera') || nativePermissionDeclared('microphone')) && typeof window.navigator?.mediaDevices?.getUserMedia === 'function'
            case 'media.getUserMedia.camera': return nativePermissionDeclared('camera') && typeof window.navigator?.mediaDevices?.getUserMedia === 'function'
            case 'media.getUserMedia.microphone': return nativePermissionDeclared('microphone') && typeof window.navigator?.mediaDevices?.getUserMedia === 'function'
            case 'file.saveText': return typeof window.document?.createElement === 'function'
            case 'capabilities.state': return typeof window.navigator?.permissions?.query === 'function'
            case 'host.events': return typeof (/** @type {any} */ (window).__tcNativeBridge__)?.onMessage === 'function'
            default: return false
        }
    }

    /** @param {string} permission */
    const nativePermissionDeclared = (permission) => {
        if (!isBrowser) return false
        const target = [...window.document.getElementsByTagName('meta')]
            .find((element) => element.getAttribute('name') === 'tachyon-target')?.getAttribute('content') ?? 'web'
        if (target === 'web') return true
        const declared = [...window.document.getElementsByTagName('meta')]
            .find((element) => element.getAttribute('name') === 'tachyon-device-permissions')?.getAttribute('content') ?? ''
        return declared.split(',').map((entry) => entry.trim()).includes(permission)
    }

    /** @param {string} capability */
    const capabilityPermissionName = (capability) => {
        switch (capability) {
            case 'geo.current': return 'geolocation'
            case 'notify.show': return 'notifications'
            case 'media.getUserMedia.camera': return 'camera'
            case 'media.getUserMedia.microphone': return 'microphone'
            default: return undefined
        }
    }

    /** @param {string} capability */
    const capabilityState = async (capability) => {
        if (deviceAvailable(capability)) {
            if (deviceAvailable('capabilities.state'))
                return await device('capabilities.state', { capability })
            return capability === 'auth.verifyUser' ? 'prompt' : 'granted'
        }
        const permissionName = capabilityPermissionName(capability)
        if (!permissionName || !isBrowser) return standardCapabilityAvailable(capability) ? 'prompt' : 'unsupported'
        try {
            const status = await window.navigator.permissions?.query(/** @type {PermissionDescriptor} */ ({ name: permissionName }))
            if (status?.state === 'granted' || status?.state === 'denied' || status?.state === 'prompt') return status.state
        }
        catch {
            // Safari and several WebViews implement the feature but do not
            // expose it through the Permissions API. The feature remains
            // requestable, so report prompt rather than pretending it is denied.
        }
        return standardCapabilityAvailable(capability) || capability === 'media.getUserMedia.camera' || capability === 'media.getUserMedia.microphone'
            ? 'prompt'
            : 'unsupported'
    }

    /** @param {string} name @param {string} text */
    const saveText = async (name, text) => {
        const picker = /** @type {any} */ (window).showSaveFilePicker
        if (typeof picker === 'function') {
            const handle = await picker({
                suggestedName: name,
                types: [{ description: 'Text files', accept: { 'text/plain': ['.txt', '.md', '.json', '.csv'] } }],
            })
            const writable = await handle.createWritable()
            await writable.write(text)
            await writable.close()
            return { name: handle.name, saved: true }
        }
        const blob = new Blob([text], { type: 'text/plain;charset=utf-8' })
        const href = URL.createObjectURL(blob)
        const anchor = window.document.createElement('a')
        anchor.href = href
        anchor.download = name
        anchor.style.display = 'none'
        window.document.body.append(anchor)
        anchor.click()
        anchor.remove()
        URL.revokeObjectURL(href)
        return { name, saved: true }
    }

    /** @param {PositionOptions | undefined} options */
    const currentPosition = async (options = undefined) => {
        if (!nativePermissionDeclared('location'))
            throw new Error('Geolocation requires the location device permission in package.json')
        if (!window.navigator?.geolocation?.getCurrentPosition)
            throw new Error('Geolocation is unavailable in this environment')
        return await new Promise((resolve, reject) => window.navigator.geolocation.getCurrentPosition(
            (position) => resolve({
                latitude: position.coords.latitude,
                longitude: position.coords.longitude,
                accuracy: position.coords.accuracy,
                altitude: position.coords.altitude,
                altitudeAccuracy: position.coords.altitudeAccuracy,
                heading: position.coords.heading,
                speed: position.coords.speed,
                timestamp: position.timestamp,
            }),
            reject,
            options,
        ))
    }

    /** @param {string} title @param {NotificationOptions | undefined} options */
    const showNotification = async (title, options = undefined) => {
        if (!nativePermissionDeclared('notifications'))
            throw new Error('Notifications require the notifications device permission in package.json')
        if (typeof window.Notification !== 'function')
            throw new Error('Notifications are unavailable in this environment')
        const permission = window.Notification.permission === 'default'
            ? await window.Notification.requestPermission()
            : window.Notification.permission
        if (permission !== 'granted') throw new Error('Notification permission was not granted')
        new window.Notification(title, options)
        return { shown: true }
    }

    /** @param {string} event @param {(payload: unknown) => void} handler */
    const subscribeHostEvent = (event, handler) => {
        const bridge = /** @type {{ onMessage?: (handler: (message: unknown) => void) => () => void } | undefined} */ (
            /** @type {any} */ (globalThis).__tcNativeBridge__ ?? (isBrowser ? /** @type {any} */ (window).__tcNativeBridge__ : undefined))
        if (typeof bridge?.onMessage !== 'function')
            throw new Error('Host events are unavailable in this environment')
        return bridge.onMessage((message) => {
            const value = /** @type {{ type?: string, event?: string, payload?: unknown }} */ (message)
            if (value?.type === 'tac:host-event' && value.event === event) handler(value.payload)
        })
    }

    /** @param {string} accelerator */
    const validateAccelerator = (accelerator) => {
        const parts = accelerator.split('+').map((part) => part.trim()).filter(Boolean)
        const modifiers = new Set(['Primary', 'Control', 'Command', 'Meta', 'Alt', 'Option', 'Shift'])
        if (parts.length < 2 || parts.slice(0, -1).some((part) => !modifiers.has(part))
            || !/^(?:[A-Z0-9]|F(?:[1-9]|1[0-9]|2[0-4]))$/i.test(parts.at(-1) ?? ''))
            throw new Error('Shortcut accelerator must use portable modifiers and one supported key')
        return parts.join('+')
    }

    /** @param {string} raw */
    const requireHttpsURL = (raw) => {
        let url
        try { url = new URL(raw) }
        catch { throw new Error('Managed content requires an HTTPS URL') }
        if (url.protocol !== 'https:') throw new Error('Managed content requires an HTTPS URL')
        return raw
    }

    /** @param {string} raw */
    const requireSurfaceId = (raw) => {
        const id = String(raw ?? '')
        if (!/^[A-Za-z0-9._-]{1,128}$/.test(id))
            throw new Error("Managed content surface id must be 1-128 ASCII letters, digits, '.', '_', or '-'")
        return id
    }

    /**
     * Permissioned device boundary shared by JavaScript companions and
     * generated language adapters. A native host may provide richer behavior;
     * browsers receive only standards-based fallbacks.
     * @param {string} capability
     * @param {unknown} [payload]
     * @returns {Promise<unknown>}
     */
    const device = async (capability, payload = {}) => {
        if (!isDeviceCapabilityAuthorized(capability)) {
            throw new Error(`Tac device capability '${capability}' is not implemented by this bundle target`)
        }
        const bridge = /** @type {{ invoke?: Function } | undefined} */ (
            /** @type {any} */ (globalThis).__tcNativeBridge__ ?? (isBrowser ? /** @type {any} */ (window).__tcNativeBridge__ : undefined))
        if (typeof bridge?.invoke === 'function')
            return await bridge.invoke(capability, payload, { source: modulePath })
        if (!isBrowser)
            throw new Error(`Tac device capability '${capability}' is not available in this environment`)
        const data = payload && typeof payload === 'object' ? /** @type {Record<string, unknown>} */ (payload) : {}
        if (capability === 'app.info') {
            return {
                name: window.document.title || 'Tachyon App',
                runtime: 'web',
                href: window.location.href,
                userAgent: window.navigator?.userAgent || '',
                language: window.navigator?.language || '',
                online: Boolean(window.navigator?.onLine),
            }
        }
        if (capability === 'clipboard.readText' && window.navigator?.clipboard?.readText)
            return await window.navigator.clipboard.readText()
        if (capability === 'clipboard.writeText' && window.navigator?.clipboard?.writeText) {
            await window.navigator.clipboard.writeText(String(data.text ?? ''))
            return { written: true }
        }
        if (capability === 'openUrl') {
            const url = String(data.url ?? '')
            if (!/^https?:\/\//i.test(url)) throw new Error('openUrl requires an http(s) URL')
            window.open(url, '_blank', 'noopener,noreferrer')
            return { opened: true }
        }
        if (capability === 'share.text' && typeof window.navigator?.share === 'function') {
            await window.navigator.share({
                text: String(data.text ?? ''),
                title: String(data.title ?? ''),
            })
            return { shared: true }
        }
        if (capability === 'haptics.impact' && typeof window.navigator?.vibrate === 'function') {
            window.navigator.vibrate(10)
            return { impacted: true }
        }
        const filePicker = /** @type {any} */ (window).showOpenFilePicker
        if (capability === 'file.openText' && typeof filePicker === 'function') {
            const [handle] = await filePicker({
                multiple: false,
                types: [{ description: 'Text files', accept: { 'text/*': ['.txt', '.md', '.json', '.csv'] } }],
            })
            const file = await handle.getFile()
            return { name: file.name, text: await file.text() }
        }
        throw new Error(`Tac device capability '${capability}' is not available in this environment`)
    }

    // A language-neutral facade over browser APIs. Generated companions can use
    // this without assuming JavaScript globals exist in their source language.
    const web = Object.freeze({
        fetch: (/** @type {RequestInfo | URL} */ input, /** @type {RequestInit | undefined} */ init) => localFirstFetch(input, init),
        storage: Object.freeze({
            local: Object.freeze({
                get: (/** @type {string} */ key, /** @type {unknown} */ fallback) => readLocalValue(key, fallback),
                set: (/** @type {string} */ key, /** @type {unknown} */ value) => writeLocalValue(key, value),
                remove: (/** @type {string} */ key) => writeLocalValue(key, undefined),
            }),
            session: Object.freeze({
                get: (/** @type {string} */ key, /** @type {unknown} */ fallback) => readSessionValue(key, fallback),
                set: (/** @type {string} */ key, /** @type {unknown} */ value) => writeSessionValue(key, value),
                remove: (/** @type {string} */ key) => writeSessionValue(key, undefined),
            }),
        }),
        navigator: Object.freeze({
            online: () => Boolean(isBrowser && window.navigator?.onLine),
            language: () => isBrowser ? (window.navigator?.language || '') : '',
            userAgent: () => isBrowser ? (window.navigator?.userAgent || '') : '',
        }),
        location: Object.freeze({
            href: () => isBrowser ? window.location.href : '',
            origin: () => isBrowser ? window.location.origin : '',
        }),
        clipboard: Object.freeze({
            readText: async () => {
                if (!isBrowser || !window.navigator?.clipboard?.readText)
                    throw new Error('Clipboard read is unavailable in this browser')
                return await window.navigator.clipboard.readText()
            },
            writeText: async (/** @type {string} */ text) => {
                if (!isBrowser || !window.navigator?.clipboard?.writeText)
                    throw new Error('Clipboard write is unavailable in this browser')
                await window.navigator.clipboard.writeText(String(text))
            },
        }),
        sleep: async (/** @type {number} */ milliseconds) => {
            await new Promise((resolve) => setTimeout(resolve, Math.max(0, Number(milliseconds) || 0)))
        },
    })

    /**
     * Resolves the bundled FYLO browser client only at call time. The compiler
     * imports its side-effect module for companions that reference `Fylo`, so
     * this works uniformly in browser and secure native-WebView builds without
     * exposing OPFS handles or Tac internals to application source.
     * @param {string} collection
     */
    const fyloCollection = (collection) => {
        const client = isBrowser ? /** @type {any} */ (window).fylo : undefined
        if (!client)
            throw new Error('FYLO is unavailable during prerender; call it from a mounted Tac companion.')
        if (typeof client.collection === 'function')
            return client.collection(String(collection))
        const facade = client[String(collection)]
        if (!facade)
            throw new Error(`FYLO collection '${String(collection)}' is unavailable`)
        return facade
    }

    /** @param {{ id: string, url: string, persistentSession?: boolean }} options */
    const openContentSurface = async (options) => {
        const id = requireSurfaceId(options?.id)
        let nativeOpenStarted = false
        /** @type {ReturnType<typeof setTimeout> | undefined} */
        let timeout
        /** @type {(value?: unknown) => void} */
        let resolveReady = () => {}
        /** @type {(error: Error) => void} */
        let rejectReady = () => {}
        const ready = new Promise((resolve, reject) => {
            resolveReady = resolve
            rejectReady = reject
        })
        const stopOpened = subscribeHostEvent('surface.opened', (payload) => {
            if (String(/** @type {any} */ (payload)?.id ?? '') === id) resolveReady()
        })
        const stopFailed = subscribeHostEvent('surface.failed', (payload) => {
            if (String(/** @type {any} */ (payload)?.id ?? '') === id)
                rejectReady(new Error('Managed content surface failed to open'))
        })
        try {
            const result = /** @type {{ pending?: boolean }} */ (await device('contentSurface.open', {
                id,
                url: requireHttpsURL(String(options?.url ?? '')),
                persistentSession: Boolean(options?.persistentSession),
            }))
            nativeOpenStarted = true
            if (result?.pending) {
                timeout = setTimeout(
                    () => rejectReady(new Error('Managed content surface timed out while opening; native creation will be cancelled')),
                    10000,
                )
                await ready
            }
            return await device('contentSurface.state', { id })
        } catch (error) {
            if (nativeOpenStarted) {
                try { await device('contentSurface.close', { id }) } catch {}
            }
            throw error
        } finally {
            if (timeout) clearTimeout(timeout)
            stopOpened()
            stopFailed()
        }
    }

    // Language compilers lower their native-shaped APIs to this internal object.
    // It deliberately reuses the public Web facade and the permissioned device
    // bridge so there is one policy implementation to audit.
    const native = Object.freeze({
        capabilities: Object.freeze({
            supports: (/** @type {string} */ capability) => {
                const portable = new Set([
                    'web.fetch', 'web.localStorage', 'web.sessionStorage',
                    'web.navigator', 'web.location', 'fylo.collection',
                ])
                return portable.has(capability) || deviceAvailable(capability) || standardCapabilityAvailable(capability)
            },
            state: capabilityState,
        }),
        app: Object.freeze({
            available: () => deviceAvailable('app.info'),
            info: () => device('app.info'),
        }),
        clipboard: Object.freeze({
            available: () => deviceAvailable('clipboard.readText') || deviceAvailable('clipboard.writeText'),
            readText: () => device('clipboard.readText'),
            writeText: (/** @type {string} */ text) => device('clipboard.writeText', { text }),
        }),
        fileSystem: Object.freeze({
            readText: (/** @type {string} */ path) => device('fs.readText', { path }),
            writeText: (/** @type {string} */ path, /** @type {string} */ text) => device('fs.writeText', { path, text }),
            readDir: (/** @type {string} */ path) => device('fs.readDir', { path }),
            stat: (/** @type {string} */ path) => device('fs.stat', { path }),
            mkdir: (/** @type {string} */ path) => device('fs.mkdir', { path }),
            remove: (/** @type {string} */ path) => device('fs.remove', { path }),
            paths: () => device('fs.paths'),
        }),
        shell: Object.freeze({
            exec: (/** @type {string} */ command, /** @type {string[]} */ args = [], /** @type {string | undefined} */ cwd = undefined) => device('shell.exec', { command, args, cwd }),
        }),
        browser: Object.freeze({
            available: () => deviceAvailable('openUrl'),
            open: (/** @type {string} */ url) => device('openUrl', { url }),
        }),
        share: Object.freeze({
            text: (/** @type {string} */ text, /** @type {string} */ title = '') => device('share.text', { text, title }),
        }),
        haptics: Object.freeze({
            impact: () => device('haptics.impact'),
        }),
        filePicker: Object.freeze({
            available: () => deviceAvailable('file.openText'),
            openText: () => device('file.openText'),
            saveText,
        }),
        secrets: Object.freeze({
            get: (/** @type {string} */ key) => device('secrets.get', { key }),
            set: async (/** @type {string} */ key, /** @type {string} */ value) => { await device('secrets.set', { key, value }) },
            delete: async (/** @type {string} */ key) => { await device('secrets.delete', { key }) },
        }),
        auth: Object.freeze({
            verifyUser: (/** @type {string} */ reason) => device('auth.verifyUser', { reason }),
        }),
        geolocation: Object.freeze({ current: currentPosition }),
        notifications: Object.freeze({ show: showNotification }),
        media: Object.freeze({
            getUserMedia: (/** @type {MediaStreamConstraints} */ constraints) => {
                if (constraints?.video && !nativePermissionDeclared('camera'))
                    throw new Error('Camera capture requires the camera device permission in package.json')
                if (constraints?.audio && !nativePermissionDeclared('microphone'))
                    throw new Error('Microphone capture requires the microphone device permission in package.json')
                if (!window.navigator?.mediaDevices?.getUserMedia)
                    throw new Error('Media capture is unavailable in this environment')
                return window.navigator.mediaDevices.getUserMedia(constraints)
            },
        }),
        host: Object.freeze({
            on: subscribeHostEvent,
            invoke: (/** @type {string} */ operation, /** @type {unknown} */ payload = {}) => device(operation, payload),
        }),
        shortcuts: Object.freeze({
            register: (/** @type {{ id: string, accelerator: string, replace?: boolean }} */ options) => device('shortcuts.register', {
                id: String(options?.id ?? ''),
                accelerator: validateAccelerator(String(options?.accelerator ?? '')),
                ...(options?.replace ? { replace: true } : {}),
            }),
            unregister: (/** @type {string} */ id) => device('shortcuts.unregister', { id: String(id) }),
            unregisterAll: () => device('shortcuts.unregisterAll'),
            list: () => device('shortcuts.list'),
        }),
        appWindow: Object.freeze({
            state: () => device('window.state'),
            setAlwaysOnTop: (/** @type {boolean} */ enabled) => device('window.alwaysOnTop', { enabled: Boolean(enabled) }),
            setOpacity: (/** @type {number} */ value) => {
                if (!Number.isFinite(value)) throw new Error('Window opacity must be finite')
                if (value < 0.1 || value > 1) throw new Error('Window opacity must be between 0.1 and 1')
                return device('window.opacity', { value })
            },
            setClickThrough: (/** @type {boolean} */ enabled) => device('window.clickThrough', { enabled: Boolean(enabled) }),
            setCaptureProtection: (/** @type {boolean} */ enabled) => device('window.captureProtection', { enabled: Boolean(enabled) }),
        }),
        contentSurface: Object.freeze({
            open: openContentSurface,
            navigate: async (/** @type {string} */ rawId, /** @type {string} */ url) => {
                const id = requireSurfaceId(rawId)
                await device('contentSurface.navigate', { id, url: requireHttpsURL(String(url)) })
                return await device('contentSurface.state', { id })
            },
            state: (/** @type {string} */ id) => device('contentSurface.state', { id: requireSurfaceId(id) }),
            goBack: async (/** @type {string} */ rawId) => {
                const id = requireSurfaceId(rawId)
                await device('contentSurface.goBack', { id })
                return await device('contentSurface.state', { id })
            },
            goForward: async (/** @type {string} */ rawId) => {
                const id = requireSurfaceId(rawId)
                await device('contentSurface.goForward', { id })
                return await device('contentSurface.state', { id })
            },
            reload: async (/** @type {string} */ rawId) => {
                const id = requireSurfaceId(rawId)
                await device('contentSurface.reload', { id })
                return await device('contentSurface.state', { id })
            },
            close: async (/** @type {string} */ rawId) => {
                const id = requireSurfaceId(rawId)
                await device('contentSurface.close', { id })
                return { id, open: false }
            },
        }),
        screenCapture: Object.freeze({
            state: () => device('screenCapture.state'),
            listWindows: (/** @type {{ visibleOnly?: boolean, excludeCurrentApp?: boolean }} */ options = {}) => device('screenCapture.listWindows', {
                visibleOnly: options.visibleOnly !== false,
                excludeCurrentApp: options.excludeCurrentApp !== false,
            }),
            captureWindow: (/** @type {{ windowId: string, destination: string, format?: string }} */ options) => {
                const destination = String(options?.destination ?? '')
                if (!['clipboard', 'file', 'both'].includes(destination))
                    throw new Error('Capture destination must be clipboard, file, or both')
                const format = String(options?.format ?? 'png').toLowerCase()
                if (format !== 'png') throw new Error('Screen capture format must be png')
                return device('screenCapture.captureWindow', { windowId: String(options?.windowId ?? ''), destination, format })
            },
        }),
        fylo: Object.freeze({
            collection: fyloCollection,
        }),
        web: Object.freeze({
            localStorage: Object.freeze({
                getItem: (/** @type {string} */ key, /** @type {unknown} */ fallback) => web.storage.local.get(key, fallback),
                setItem: (/** @type {string} */ key, /** @type {unknown} */ value) => web.storage.local.set(key, value),
                removeItem: (/** @type {string} */ key) => web.storage.local.remove(key),
            }),
            sessionStorage: Object.freeze({
                getItem: (/** @type {string} */ key, /** @type {unknown} */ fallback) => web.storage.session.get(key, fallback),
                setItem: (/** @type {string} */ key, /** @type {unknown} */ value) => web.storage.session.set(key, value),
                removeItem: (/** @type {string} */ key) => web.storage.session.remove(key),
            }),
            navigator: web.navigator,
            location: web.location,
            fetch: web.fetch,
        }),
    })

    /** @param {string} operation @param {unknown} [payload] */
    const nativeCall = async (operation, payload = {}) => {
        const data = payload && typeof payload === 'object' ? /** @type {Record<string, unknown>} */ (payload) : {}
        switch (operation) {
            case 'app.info': return await native.app.info()
            case 'clipboard.readText': return await native.clipboard.readText()
            case 'clipboard.writeText': return await native.clipboard.writeText(String(data.text ?? ''))
            case 'fs.readText': return await native.fileSystem.readText(String(data.path ?? ''))
            case 'fs.writeText': return await native.fileSystem.writeText(String(data.path ?? ''), String(data.text ?? ''))
            case 'fs.readDir': return await native.fileSystem.readDir(String(data.path ?? ''))
            case 'fs.stat': return await native.fileSystem.stat(String(data.path ?? ''))
            case 'fs.mkdir': return await native.fileSystem.mkdir(String(data.path ?? ''))
            case 'fs.remove': return await native.fileSystem.remove(String(data.path ?? ''))
            case 'fs.paths': return await native.fileSystem.paths()
            case 'shell.exec': return await native.shell.exec(String(data.command ?? ''), Array.isArray(data.args) ? data.args.map(String) : [], typeof data.cwd === 'string' ? data.cwd : undefined)
            case 'browser.open': return await native.browser.open(String(data.url ?? ''))
            case 'share.text': return await native.share.text(String(data.text ?? ''), String(data.title ?? ''))
            case 'haptics.impact': return await native.haptics.impact()
            case 'filePicker.openText': return await native.filePicker.openText()
            case 'filePicker.saveText': return await native.filePicker.saveText(String(data.name ?? 'untitled.txt'), String(data.text ?? ''))
            case 'secrets.get': return await native.secrets.get(String(data.key ?? ''))
            case 'secrets.set': return await native.secrets.set(String(data.key ?? ''), String(data.value ?? ''))
            case 'secrets.delete': return await native.secrets.delete(String(data.key ?? ''))
            case 'auth.verifyUser': return await native.auth.verifyUser(String(data.reason ?? 'Verify your identity'))
            case 'geo.current': return await native.geolocation.current(/** @type {PositionOptions | undefined} */ (data.options))
            case 'notify.show': return await native.notifications.show(String(data.title ?? ''), /** @type {NotificationOptions | undefined} */ (data.options))
            case 'media.getUserMedia': return await native.media.getUserMedia(/** @type {MediaStreamConstraints} */ (data.constraints))
            case 'capabilities.state': return await native.capabilities.state(String(data.capability ?? ''))
            case 'shortcuts.register': return await native.shortcuts.register(/** @type {any} */ (data))
            case 'shortcuts.unregister': return await native.shortcuts.unregister(String(data.id ?? ''))
            case 'shortcuts.unregisterAll': return await native.shortcuts.unregisterAll()
            case 'shortcuts.list': return await native.shortcuts.list()
            case 'window.state': return await native.appWindow.state()
            case 'window.alwaysOnTop': return await native.appWindow.setAlwaysOnTop(Boolean(data.enabled))
            case 'window.opacity': return await native.appWindow.setOpacity(Number(data.value))
            case 'window.clickThrough': return await native.appWindow.setClickThrough(Boolean(data.enabled))
            case 'window.captureProtection': return await native.appWindow.setCaptureProtection(Boolean(data.enabled))
            case 'contentSurface.open': return await native.contentSurface.open(/** @type {any} */ (data))
            case 'contentSurface.navigate': return await native.contentSurface.navigate(String(data.id ?? ''), String(data.url ?? ''))
            case 'contentSurface.state': return await native.contentSurface.state(String(data.id ?? ''))
            case 'contentSurface.goBack': return await native.contentSurface.goBack(String(data.id ?? ''))
            case 'contentSurface.goForward': return await native.contentSurface.goForward(String(data.id ?? ''))
            case 'contentSurface.reload': return await native.contentSurface.reload(String(data.id ?? ''))
            case 'contentSurface.close': return await native.contentSurface.close(String(data.id ?? ''))
            case 'screenCapture.state': return await native.screenCapture.state()
            case 'screenCapture.listWindows': return await native.screenCapture.listWindows(/** @type {any} */ (data))
            case 'screenCapture.captureWindow': return await native.screenCapture.captureWindow(/** @type {any} */ (data))
            case 'web.fetch': {
                const input = String(data.input ?? data.url ?? '')
                if (!input) throw new Error('web.fetch requires a request URL')
                return await native.web.fetch(input, /** @type {RequestInit | undefined} */ (data.init))
            }
            case 'web.localStorage.getItem': return native.web.localStorage.getItem(String(data.key ?? ''), data.fallback)
            case 'web.localStorage.setItem': return native.web.localStorage.setItem(String(data.key ?? ''), data.value)
            case 'web.localStorage.removeItem': return native.web.localStorage.removeItem(String(data.key ?? ''))
            case 'web.sessionStorage.getItem': return native.web.sessionStorage.getItem(String(data.key ?? ''), data.fallback)
            case 'web.sessionStorage.setItem': return native.web.sessionStorage.setItem(String(data.key ?? ''), data.value)
            case 'web.sessionStorage.removeItem': return native.web.sessionStorage.removeItem(String(data.key ?? ''))
            case 'web.navigator.language': return native.web.navigator.language()
            case 'web.navigator.online': return native.web.navigator.online()
            case 'web.location.href': return native.web.location.href()
            case 'web.location.origin': return native.web.location.origin()
            case 'fylo.collection.find':
            case 'fylo.collection.get':
            case 'fylo.collection.create':
            case 'fylo.collection.patch':
            case 'fylo.collection.del':
            case 'fylo.collection.delete':
            case 'fylo.collection.list':
            case 'fylo.collection.put':
            case 'fylo.collection.restore':
            case 'fylo.collection.latest':
            case 'fylo.collection.inspect':
            case 'fylo.collection.rebuild': {
                const method = operation.slice('fylo.collection.'.length)
                const resolvedMethod = method === 'delete' ? 'del' : method
                const facade = fyloCollection(String(data.collection ?? ''))
                const invoke = facade[resolvedMethod]
                if (typeof invoke !== 'function')
                    throw new Error(`FYLO collection method '${resolvedMethod}' is unavailable`)
                const args = Array.isArray(data.args) ? data.args : []
                return await invoke.apply(facade, args)
            }
            case 'capabilities.supports': return native.capabilities.supports(String(data.capability ?? ''))
            default:
                if (native.capabilities.supports(operation)) return await native.host.invoke(operation, data)
                throw new Error(`Unknown native shim operation '${operation}'`)
        }
    }

    /** @param {string} operation */
    const nativeAvailable = (operation) => {
        switch (operation) {
            case 'app.info': return native.app.available()
            case 'clipboard.readText':
            case 'clipboard.writeText': return native.clipboard.available()
            case 'fs.readText':
            case 'fs.writeText':
            case 'fs.readDir':
            case 'fs.stat':
            case 'fs.mkdir':
            case 'fs.remove':
            case 'fs.paths':
            case 'shell.exec': return deviceAvailable(operation)
            case 'browser.open': return native.browser.available()
            case 'share.text':
            case 'haptics.impact': return deviceAvailable(operation)
            case 'filePicker.openText': return native.filePicker.available()
            case 'filePicker.saveText': return standardCapabilityAvailable('file.saveText')
            case 'secrets.get':
            case 'secrets.set':
            case 'secrets.delete':
            case 'auth.verifyUser': return deviceAvailable(operation)
            case 'geo.current': return standardCapabilityAvailable('geo.current')
            case 'notify.show': return standardCapabilityAvailable('notify.show')
            case 'media.getUserMedia': return standardCapabilityAvailable('media.getUserMedia')
            case 'capabilities.state': return standardCapabilityAvailable('capabilities.state')
            case 'shortcuts.register':
            case 'shortcuts.unregister':
            case 'shortcuts.unregisterAll':
            case 'shortcuts.list':
            case 'window.state':
            case 'window.alwaysOnTop':
            case 'window.opacity':
            case 'window.clickThrough':
            case 'window.captureProtection':
            case 'contentSurface.open':
            case 'contentSurface.navigate':
            case 'contentSurface.state':
            case 'contentSurface.goBack':
            case 'contentSurface.goForward':
            case 'contentSurface.reload':
            case 'contentSurface.close':
            case 'screenCapture.state':
            case 'screenCapture.listWindows':
            case 'screenCapture.captureWindow': return deviceAvailable(operation)
            default: return false
        }
    }

    /** @param {string} operation @param {string | undefined} payload @param {(value: unknown) => void} resolve @param {(reason: unknown) => void} reject */
    const nativeCallback = (operation, payload, resolve, reject) => {
        let decoded = /** @type {unknown} */ ({})
        try {
            decoded = payload ? JSON.parse(payload) : {}
        }
        catch (error) {
            reject(error)
            return
        }
        void nativeCall(operation, decoded).then(resolve, reject)
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
        __native: native,
        __nativeCall: nativeCall,
        __nativeAvailable: nativeAvailable,
        __nativeCallback: nativeCallback,
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
