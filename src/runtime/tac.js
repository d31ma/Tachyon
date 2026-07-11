// @ts-check
/**
 * @typedef {Record<string, unknown>} TacProps
 */

/**
 * @template T
 * @callback SignalSubscribeHelper
 * @param {string} name
 * @param {((value: unknown) => void | Promise<void>) | T} [callbackOrFallback]
 * @param {TacSignalSubscribeOptions} [options]
 * @returns {(() => void) | T | undefined}
 */

/**
 * @typedef {object} TacSignalPublishOptions
 * @property {boolean} [retain]
 */

/**
 * @typedef {object} TacSignalSubscribeOptions
 * @property {boolean} [immediate]
 */

/**
 * Rooted terminology: `platform` is the form factor (desktop | mobile | web);
 * `environment` and `os` are synonyms for the concrete host
 * (windows | macos | linux | android | ios | web); `target` is the bundle output.
 * @typedef {object} TacPlatformContext
 * @property {'web' | 'macos' | 'windows' | 'linux' | 'android' | 'ios'} target
 * @property {'desktop' | 'mobile' | 'web'} platform
 * @property {'macos' | 'windows' | 'linux' | 'android' | 'ios' | 'web'} environment
 * @property {'macos' | 'windows' | 'linux' | 'android' | 'ios' | 'web'} os
 * @property {'macos' | 'windows' | 'linux' | 'android' | 'ios' | 'unknown'} [browserOS]
 * @property {boolean} native
 * @property {boolean} web
 * @property {boolean} desktop
 * @property {boolean} mobile
 */

/**
 * @typedef {object} TacRuntimeBindings
 * @property {boolean} isBrowser
 * @property {boolean} isServer
 * @property {(controller: Record<string, unknown>) => void} bindPersistentFields
 * @property {<T>(key: string, fallback?: T) => T | undefined} env
 * @property {TacPlatformContext} platform
 * @property {TacProps} props
 * @property {(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>} fetch
 * @property {TacNativeBindings} __native
 * @property {(operation: string, payload?: unknown) => Promise<unknown>} __nativeCall
 * @property {(operation: string) => boolean} __nativeAvailable
 * @property {(operation: string, payload: string | undefined, resolve: (value: unknown) => void, reject: (reason: unknown) => void) => void} __nativeCallback
 * @property {(fn: () => void | Promise<void>) => void} onMount
 * @property {(name: string, value?: unknown, options?: TacSignalPublishOptions) => boolean} publish
 * @property {() => void} rerender
 * @property {SignalSubscribeHelper<unknown>} subscribe
 */

/**
 * Browser-safe Web API facade used internally by language-native shims.
 * @typedef {object} TacWebBindings
 * @property {(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>} fetch
 * @property {{ local: { get: <T>(key: string, fallback?: T) => T | undefined, set: (key: string, value: unknown) => void, remove: (key: string) => void }, session: { get: <T>(key: string, fallback?: T) => T | undefined, set: (key: string, value: unknown) => void, remove: (key: string) => void } }} storage
 * @property {{ online: () => boolean, language: () => string, userAgent: () => string }} navigator
 * @property {{ href: () => string, origin: () => string }} location
 * @property {{ readText: () => Promise<string>, writeText: (text: string) => Promise<void> }} clipboard
 * @property {(milliseconds: number) => Promise<void>} sleep
 */

/**
 * Internal bridge used by generated language-native shims. App authors use
 * an implicit language prelude such as `clipboard`, `localStorage`, and
 * `local_storage` instead.
 * @typedef {object} TacNativeBindings
 * @property {{ supports: (capability: string) => boolean }} capabilities
 * @property {{ available: () => boolean, info: () => Promise<unknown> }} app
 * @property {{ available: () => boolean, readText: () => Promise<unknown>, writeText: (text: string) => Promise<unknown> }} clipboard
 * @property {{ readText: (path: string) => Promise<unknown>, writeText: (path: string, text: string) => Promise<unknown>, readDir: (path: string) => Promise<unknown> }} fileSystem
 * @property {{ exec: (command: string, args?: string[], cwd?: string) => Promise<unknown> }} shell
 * @property {{ available: () => boolean, open: (url: string) => Promise<unknown> }} browser
 * @property {{ text: (text: string, title?: string) => Promise<unknown> }} share
 * @property {{ impact: () => Promise<unknown> }} haptics
 * @property {{ available: () => boolean, openText: () => Promise<unknown> }} filePicker
 * @property {{ localStorage: { getItem: <T>(key: string, fallback?: T) => T | undefined, setItem: (key: string, value: unknown) => void, removeItem: (key: string) => void }, sessionStorage: { getItem: <T>(key: string, fallback?: T) => T | undefined, setItem: (key: string, value: unknown) => void, removeItem: (key: string) => void }, navigator: { language: () => string, online: () => boolean, userAgent: () => string }, location: { href: () => string, origin: () => string }, fetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response> }} web
 */

/** @type {TacRuntimeBindings} */
const noopHelpers = {
    isBrowser: false,
    isServer: true,
    bindPersistentFields: () => { },
    env: (_, fallback) => fallback,
    platform: Object.freeze({
        target: 'web',
        platform: 'web',
        environment: 'web',
        os: 'web',
        browserOS: 'unknown',
        native: false,
        web: true,
        desktop: false,
        mobile: false,
    }),
    props: {},
    fetch: (input, init) => fetch(input, init),
    __native: /** @type {TacNativeBindings} */ ({
        capabilities: { supports: () => false },
        app: { available: () => false, info: async () => { throw new Error('Native app APIs are unavailable outside a bundled Tac app'); } },
        clipboard: { available: () => false, readText: async () => { throw new Error('Native clipboard APIs are unavailable outside a bundled Tac app'); }, writeText: async () => { throw new Error('Native clipboard APIs are unavailable outside a bundled Tac app'); } },
        fileSystem: { readText: async () => { throw new Error('Native filesystem APIs are unavailable outside a bundled Tac app'); }, writeText: async () => { throw new Error('Native filesystem APIs are unavailable outside a bundled Tac app'); }, readDir: async () => { throw new Error('Native filesystem APIs are unavailable outside a bundled Tac app'); } },
        shell: { exec: async () => { throw new Error('Native shell APIs are unavailable outside a bundled Tac app'); } },
        browser: { available: () => false, open: async () => { throw new Error('Native browser APIs are unavailable outside a bundled Tac app'); } },
        share: { text: async () => { throw new Error('Native share APIs are unavailable outside a bundled Tac app'); } },
        haptics: { impact: async () => { throw new Error('Native haptics APIs are unavailable outside a bundled Tac app'); } },
        filePicker: { available: () => false, openText: async () => { throw new Error('Native file APIs are unavailable outside a bundled Tac app'); } },
        web: {
            localStorage: { getItem: (_key, fallback) => fallback, setItem: () => {}, removeItem: () => {} },
            sessionStorage: { getItem: (_key, fallback) => fallback, setItem: () => {}, removeItem: () => {} },
            navigator: { language: () => '', online: () => false, userAgent: () => '' },
            location: { href: () => '', origin: () => '' },
            fetch: (input, init) => fetch(input, init),
        },
    }),
    __nativeCall: async (operation) => { throw new Error(`Native shim operation '${operation}' is unavailable outside a bundled Tac app`); },
    __nativeAvailable: () => false,
    __nativeCallback: (_operation, _payload, _resolve, reject) => reject(new Error('Native shims are unavailable outside a bundled Tac app')),
    onMount: () => { },
    publish: () => false,
    rerender: () => { },
    subscribe: (_name, callbackOrFallback) => typeof callbackOrFallback === 'function' ? () => { } : callbackOrFallback,
};

export default class Tac {
    /** @type {TacProps} */
    props;
    /** @type {TacRuntimeBindings} */
    tac;

    /**
     * @param {TacProps} [props]
     * @param {TacRuntimeBindings} [tac]
     */
    constructor(props = {}, tac = noopHelpers) {
        this.props = props;
        this.tac = tac;
    }
}
