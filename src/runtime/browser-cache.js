// @ts-check

/**
 * @typedef {'cache-first' | 'network-first' | 'reload' | 'no-store'} TacCachePolicy
 * @typedef {{ cache?: TacCachePolicy, key?: string | null, invalidateKeys?: string[], invalidatePrefixes?: string[] }} TacCacheFetchOptions
 * @typedef {{ key: string, status: number, statusText: string, headers: Record<string, string>, body: ArrayBuffer, updatedAt: number }} CachedResponseEntry
 */

const DB_NAME = 'tachyon-fetch-cache';
const DB_VERSION = 1;
const STORE_NAME = 'responses';

export class TacBrowserCache {
    constructor() {
        /** @type {Promise<IDBDatabase | null> | null} */
        this.dbPromise = null;
    }

    /** @returns {boolean} */
    isBrowser() {
        return typeof window !== 'undefined'
            && !/** @type {Record<string, unknown>} */ (globalThis).__ty_prerender__
            && typeof indexedDB !== 'undefined';
    }

    /** @returns {Promise<IDBDatabase | null>} */
    async open() {
        if (!this.isBrowser())
            return null;

        const win = /** @type {Window & { __ty_fetch_cache_db__?: IDBDatabase | null }} */ (window);
        if (win.__ty_fetch_cache_db__)
            return win.__ty_fetch_cache_db__;
        if (this.dbPromise)
            return this.dbPromise;

        this.dbPromise = new Promise((resolve) => {
            const request = indexedDB.open(DB_NAME, DB_VERSION);
            request.onupgradeneeded = () => {
                if (!request.result.objectStoreNames.contains(STORE_NAME))
                    request.result.createObjectStore(STORE_NAME, { keyPath: 'key' });
            };
            request.onsuccess = () => {
                win.__ty_fetch_cache_db__ = request.result;
                resolve(request.result);
            };
            request.onerror = () => resolve(null);
            request.onblocked = () => resolve(null);
        });

        return this.dbPromise;
    }

    /**
     * @param {Request} request
     * @param {string} [namespace]
     * @returns {string}
     */
    keyForRequest(request, namespace = 'fetch') {
        return `${namespace}:${request.method.toUpperCase()}:${request.url}`;
    }

    /**
     * @param {string} key
     * @returns {Promise<Response | null>}
     */
    async read(key) {
        const db = await this.open();
        if (!db)
            return null;

        return await new Promise((resolve) => {
            const tx = db.transaction(STORE_NAME, 'readonly');
            const request = tx.objectStore(STORE_NAME).get(key);
            request.onsuccess = () => {
                const entry = /** @type {CachedResponseEntry | undefined} */ (request.result);
                if (!entry) {
                    resolve(null);
                    return;
                }
                resolve(new Response(entry.body ? new Uint8Array(entry.body) : null, {
                    status: entry.status,
                    statusText: entry.statusText,
                    headers: entry.headers,
                }));
            };
            request.onerror = () => resolve(null);
        });
    }

    /**
     * @param {string} key
     * @param {Response} response
     * @returns {Promise<void>}
     */
    async write(key, response) {
        const db = await this.open();
        if (!db)
            return;

        const body = await response.arrayBuffer();
        await new Promise((resolve) => {
            const tx = db.transaction(STORE_NAME, 'readwrite');
            tx.oncomplete = () => resolve(undefined);
            tx.onerror = () => resolve(undefined);
            tx.objectStore(STORE_NAME).put({
                key,
                status: response.status,
                statusText: response.statusText,
                headers: Object.fromEntries(response.headers.entries()),
                body,
                updatedAt: Date.now(),
            });
        });
    }

    /**
     * @param {string} key
     * @returns {Promise<void>}
     */
    async delete(key) {
        const db = await this.open();
        if (!db)
            return;

        await new Promise((resolve) => {
            const tx = db.transaction(STORE_NAME, 'readwrite');
            tx.oncomplete = () => resolve(undefined);
            tx.onerror = () => resolve(undefined);
            tx.objectStore(STORE_NAME).delete(key);
        });
    }

    /**
     * @param {string} prefix
     * @returns {Promise<void>}
     */
    async deleteByPrefix(prefix) {
        const db = await this.open();
        if (!db)
            return;

        await new Promise((resolve) => {
            const tx = db.transaction(STORE_NAME, 'readwrite');
            const store = tx.objectStore(STORE_NAME);
            tx.oncomplete = () => resolve(undefined);
            tx.onerror = () => resolve(undefined);

            if (typeof store.openCursor !== 'function') {
                resolve(undefined);
                return;
            }

            const cursorRequest = store.openCursor();
            cursorRequest.onsuccess = () => {
                const cursor = cursorRequest.result;
                if (!cursor)
                    return;
                const key = String(cursor.key);
                if (key.startsWith(prefix))
                    cursor.delete();
                cursor.continue();
            };
            cursorRequest.onerror = () => resolve(undefined);
        });
    }

    /**
     * @param {string[]} keys
     * @param {string[]} prefixes
     * @returns {Promise<void>}
     */
    async invalidate(keys = [], prefixes = []) {
        await Promise.all([
            ...keys.map((key) => this.delete(key)),
            ...prefixes.map((prefix) => this.deleteByPrefix(prefix)),
        ]);
    }

    /**
     * @param {RequestInfo | URL} input
     * @param {RequestInit} [init]
     * @param {TacCacheFetchOptions} [options]
     * @returns {Promise<Response>}
     */
    async fetch(input, init = {}, options = {}) {
        const request = new Request(input, init);
        const method = request.method.toUpperCase();
        const policy = options.cache ?? this.policyFromRequest(request);
        const canCacheRead = this.isBrowser()
            && (method === 'GET' || method === 'HEAD')
            && policy !== 'no-store';
        const cacheKey = canCacheRead
            ? options.key ?? this.keyForRequest(request)
            : null;

        if (cacheKey && policy === 'cache-first') {
            const cached = await this.read(cacheKey);
            if (cached)
                return cached;
        }

        try {
            const nativeFetch = /** @type {typeof fetch} */ (
                /** @type {Record<string, unknown>} */ (globalThis).__ty_native_fetch__ ?? fetch
            );
            const response = await nativeFetch(input, init);
            if (cacheKey && response.ok && policy !== 'no-store')
                void this.write(cacheKey, response.clone());
            if (!cacheKey && response.ok && this.isBrowser())
                await this.invalidate(options.invalidateKeys, options.invalidatePrefixes);
            return response;
        } catch (error) {
            if (cacheKey && policy !== 'reload' && policy !== 'no-store') {
                const cached = await this.read(cacheKey);
                if (cached)
                    return cached;
            }
            throw error;
        }
    }

    /**
     * @param {Request} request
     * @returns {TacCachePolicy}
     */
    policyFromRequest(request) {
        if (request.cache === 'reload') return 'reload';
        if (request.cache === 'no-store') return 'no-store';
        return 'cache-first';
    }
}

/**
 * @returns {TacBrowserCache}
 */
export function getTacBrowserCache() {
    const g = /** @type {Record<string, unknown>} */ (globalThis);
    if (g.__ty_browser_cache__ instanceof TacBrowserCache)
        return g.__ty_browser_cache__;
    const cache = new TacBrowserCache();
    g.__ty_browser_cache__ = cache;
    return cache;
}

export const tacBrowserCache = getTacBrowserCache();
