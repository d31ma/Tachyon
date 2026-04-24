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
     * @param {TacRuntimeBindings} [helpers]
     */
    constructor(props = {}, helpers = noopHelpers) {
        this.props = props;
        this.tac = helpers;
        this.__bindTacPersistence__();
    }

    /** @param {TacRuntimeBindings} helpers */
    __attachTacHelpers__(helpers) {
        this.tac = helpers;
        this.__bindTacPersistence__();
    }

    __bindTacPersistence__() {
        this.tac.bindPersistentFields(/** @type {Record<string, unknown>} */ (this));
    }
    get isBrowser() {
        return this.tac.isBrowser;
    }
    get isServer() {
        return this.tac.isServer;
    }

    /**
     * @template T
     * @param {string} key
     * @param {T} [fallback]
     * @returns {T | undefined}
     */
    env(key, fallback) {
        return this.tac.env(key, fallback);
    }

    /**
     * @param {string} name
     * @param {unknown} [detail]
     * @returns {boolean}
     */
    emit(name, detail) {
        return this.tac.emit(name, detail);
    }

    /**
     * @param {RequestInfo | URL} input
     * @param {RequestInit} [init]
     * @returns {Promise<Response>}
     */
    fetch(input, init) {
        return this.tac.fetch(input, init);
    }

    /**
     * @template T
     * @param {string} key
     * @param {T} [fallback]
     * @returns {T | undefined}
     */
    inject(key, fallback) {
        return /** @type {T | undefined} */ (this.tac.inject(key, fallback));
    }

    /** @param {() => void | Promise<void>} fn */
    onMount(fn) {
        this.tac.onMount(fn);
    }

    /**
     * @param {string} key
     * @param {unknown} value
     */
    provide(key, value) {
        this.tac.provide(key, value);
    }
    rerender() {
        this.tac.rerender();
    }
}
