// @ts-check
/**
 * @typedef {Record<string, unknown>} TacProps
 */

/**
 * @template T
 * @callback InjectHelper
 * @param {string} key
 * @param {T} [fallback]
 * @returns {T | undefined}
 */

/**
 * @typedef {object} TacRuntimeBindings
 * @property {boolean} isBrowser
 * @property {boolean} isServer
 * @property {(controller: Record<string, unknown>) => void} bindPersistentFields
 * @property {<T>(key: string, fallback?: T) => T | undefined} env
 * @property {TacProps} props
 * @property {(name: string, detail?: unknown) => boolean} emit
 * @property {(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>} fetch
 * @property {InjectHelper<unknown>} inject
 * @property {(fn: () => void | Promise<void>) => void} onMount
 * @property {(key: string, value: unknown) => void} provide
 * @property {() => void} rerender
 */

/** @type {TacRuntimeBindings} */
const noopHelpers = {
    isBrowser: false,
    isServer: true,
    bindPersistentFields: () => { },
    env: (_, fallback) => fallback,
    props: {},
    emit: () => false,
    fetch: (input, init) => fetch(input, init),
    inject: (_, fallback) => fallback,
    onMount: () => { },
    provide: () => { },
    rerender: () => { },
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
