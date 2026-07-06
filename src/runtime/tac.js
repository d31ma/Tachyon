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
 * @typedef {object} TacPlatformContext
 * @property {'web' | 'macos' | 'windows' | 'linux' | 'android' | 'ios'} target
 * @property {'web' | 'macos' | 'windows' | 'linux' | 'android' | 'ios'} platform
 * @property {'browser' | 'desktop' | 'mobile'} environment
 * @property {'macos' | 'windows' | 'linux' | 'android' | 'ios' | 'unknown'} os
 * @property {'macos' | 'windows' | 'linux' | 'android' | 'ios' | 'unknown'} [browserOS]
 * @property {boolean} native
 * @property {boolean} browser
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
 * @property {(fn: () => void | Promise<void>) => void} onMount
 * @property {(name: string, value?: unknown, options?: TacSignalPublishOptions) => boolean} publish
 * @property {() => void} rerender
 * @property {SignalSubscribeHelper<unknown>} subscribe
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
        environment: 'browser',
        os: 'unknown',
        browserOS: 'unknown',
        native: false,
        browser: true,
        web: true,
        desktop: false,
        mobile: false,
    }),
    props: {},
    fetch: (input, init) => fetch(input, init),
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
