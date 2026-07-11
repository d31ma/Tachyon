// @ts-check
//
// Fylo browser sync client — wraps the vendored fylo web shim's local-first
// OPFS database and adds HTTP sync, IndexedDB caching, and SSE subscriptions to
// bridge the browser client with the server gateway at /_fylo.
//
// Preserves the exact API surface Tachyon companion scripts depend on:
//   fylo.<collection>.find/get/create/patch/del/subscribe
//   fylo.<collection>.batchPut/patchMany/deleteMany/restore/latest/inspect/rebuild
//   fylo.createCollection/dropCollection/inspectCollection/rebuildCollection
//   fylo.collections() / fylo.meta() / fylo.setCredentials() / fylo.sql()
import { createBrowserClient } from '../vendor/fylo/fylo-web.mjs';
import { tacBrowserCache } from './browser-cache.js';

/**
 * @typedef {'cache-first' | 'network-first' | 'reload' | 'no-store'} FyloCachePolicy
 * @typedef {{ cache?: FyloCachePolicy }} FyloQueryOptions
 * @typedef {{ collection?: string, events?: unknown[], offset?: number, exists?: boolean, error?: string }} FyloEventsPayload
 * @typedef {'initial' | 'event-stream' | 'poll' | 'local'} FyloSubscribeSource
 * @typedef {{ collection: string, events: unknown[], offset: number, source: FyloSubscribeSource }} FyloSubscribeMeta
 * @typedef {(payload: unknown, meta: FyloSubscribeMeta) => void | Promise<void>} FyloSubscribeCallback
 * @typedef {FyloQueryOptions & { pollMs?: number, since?: number, onError?: (error: unknown) => void }} FyloSubscribeOptions
 *
 * @typedef {Record<string, unknown> & { limit?: number, order?: string, select?: string }} FyloQuery
 * @typedef {{ id: string, doc: unknown }} FyloDoc
 * @typedef {{ docs: FyloDoc[], collection?: string, encryptedFields?: unknown, local?: boolean }} FyloFindResult
 * @typedef {{ find: Function, get: Function, create: Function, patch: Function, del: Function, list: Function, put: Function, subscribe: Function, events: Function, batchPut: Function, patchMany: Function, deleteMany: Function, restore: Function, latest: Function, inspect: Function, rebuild: Function, createCollection: Function, dropCollection: Function }} FyloCollection
 * @typedef {Record<string, any> & {
 *   enabled: boolean,
 *   root?: string,
 *   collections(): Promise<any>,
 *   meta(): Promise<any>,
 *   setCredentials(user: string, pass: string): void,
 *   clearCredentials(): void,
 *   sql(strings: TemplateStringsArray, ...values: unknown[]): Promise<any>,
 *   request(apiPath: string, init?: RequestInit): Promise<Response>,
 *   collection(collection: string): FyloCollection,
 *   createCollection(collection: string): Promise<{ ok?: boolean, error?: string }>,
 *   dropCollection(collection: string): Promise<{ ok?: boolean, error?: string }>,
 *   inspectCollection(collection: string): Promise<unknown>,
 *   rebuildCollection(collection: string): Promise<unknown>,
 *   onBrowserEvent(handler: (event: { id: string, ts: number, action: string, collection: string, doc?: Record<string, any> }) => void): void,
 * }} FyloApi
 */

const FYLO_BROWSER_PATH = () => {
    try {
        if (typeof document !== 'undefined' && document.head) {
            const meta = document.querySelector('meta[name="fylo-browser-path"]');
            if (meta) return meta.getAttribute('content') || '/_fylo';
        }
    } catch {
        // document API may not be available (SSR, test environments)
    }
    return '/_fylo';
};

/**
 * Creates the Tachyon fylo browser sync client (synchronous factory).
 * The vendored fylo web shim's OPFS client is lazily initialized on
 * first mutation or local read.
 *
 * @param {string} [basePath] - Path to the Fylo browser gateway (e.g. '/_fylo').
 *   Falls back to `<meta name="fylo-browser-path">` or '/_fylo'.
 * @returns {{ proxy: FyloApi, probe: () => Promise<void> }}
 */
export function createFyloClient(basePath) {
    const browserPath = basePath || FYLO_BROWSER_PATH();

    // ── Official local-first OPFS database (lazy-init) ────────────────
    /** @type {ReturnType<typeof createBrowserClient> | null} */
    let localClient = null;
    /** @type {boolean} */
    let localClientFailed = false;

    /** @returns {Promise<ReturnType<typeof createBrowserClient> | null>} */
    async function getLocalClient() {
        if (localClient) return localClient;
        if (localClientFailed) return null;
        // Skip local OPFS client when browser storage APIs are unavailable (tests, SSR)
        if (typeof navigator === 'undefined' || !navigator.storage) {
            localClientFailed = true;
            return null;
        }
        try {
            // ponytail: worker:false — the packaged worker entry resolves
            // `new URL('./shared.js', import.meta.url)` relative to the bundled
            // companion module, which 404s and leaves ready() waiting forever.
            // Run the OPFS engine on the main thread until the runtime wires the
            // emitted fylo-browser-worker.js asset URL through client options.
            localClient = createBrowserClient({
                storage: 'opfs',
                worker: false,
                namespace: 'tachyon',
            });
            await Promise.race([
                localClient.ready(),
                new Promise((_, reject) => setTimeout(() => reject(new Error('FYLO OPFS client timed out')), 4000)),
            ]);
            return localClient;
        } catch {
            try {
                localClient = createBrowserClient({
                    storage: 'memory',
                    worker: false,
                    namespace: 'tachyon',
                });
                await localClient.ready();
                return localClient;
            } catch {
                localClientFailed = true;
                localClient = null;
                return null;
            }
        }
    }

    /** @param {string} collection @returns {Promise<import('../vendor/fylo/fylo-web.mjs').BrowserDirectCollection | null>} */
    async function getLocalCollection(collection) {
        const client = await getLocalClient();
        return client ? client.collection(collection) : null;
    }

    /**
     * Query the local mirror, treating a missing local collection as empty —
     * collections only exist locally after the first mirrored write.
     * @param {import('../vendor/fylo/fylo-web.mjs').BrowserDirectCollection | null} col
     * @param {Record<string, unknown>} query
     * @returns {Promise<Array<{ id: string, doc: unknown }>>}
     */
    async function localDocs(col, query) {
        /** @type {Array<{ id: string, doc: unknown }>} */
        const docs = [];
        if (!col) return docs;
        try {
            for await (const entry of col.find(query).collect()) {
                for (const [id, doc] of Object.entries(/** @type {Record<string, unknown>} */ (entry))) {
                    docs.push({ id, doc });
                }
            }
        } catch {
            /* nonexistent local collection reads as empty */
        }
        return docs;
    }

    /**
     * Write to the local mirror, creating the collection on first use.
     * @param {import('../vendor/fylo/fylo-web.mjs').BrowserDirectCollection | null} col
     * @param {(c: import('../vendor/fylo/fylo-web.mjs').BrowserDirectCollection) => Promise<any>} write
     * @returns {Promise<any>}
     */
    async function localWrite(col, write) {
        if (!col) return undefined;
        try {
            return await write(col);
        } catch {
            try {
                await col.create();
                return await write(col);
            } catch {
                return undefined;
            }
        }
    }

    /** @type {string | null} */
    let basicAuth = null;
    /** @type {(event: { id: string, ts: number, action: string, collection: string, doc?: Record<string, any> }) => void} */
    let onBrowserEvent = () => {};

    // Resolve a (possibly relative) gateway path to an absolute URL. Browsers
    // resolve relative URLs against the page origin implicitly, but `Request`/
    // `fetch` outside a browser (SSR, workers, tests) reject relative URLs, so
    // make the origin explicit. In-browser the result is identical to the
    // implicit resolution against `location.origin`.
    /** @param {string} pathOrUrl @returns {string} */
    function absoluteUrl(pathOrUrl) {
        if (/^[a-z][a-z0-9+.-]*:\/\//i.test(pathOrUrl)) return pathOrUrl;
        let origin = 'http://localhost';
        try {
            const loc = (typeof window !== 'undefined' && window.location) ? window.location
                : (typeof location !== 'undefined' ? location : null);
            if (loc && loc.origin && loc.origin !== 'null') origin = loc.origin;
        } catch { /* no location available */ }
        return new URL(pathOrUrl, origin).href;
    }

    // ── HTTP request helper ───────────────────────────────────────────
    // Set when the /_fylo gateway answers 404 (frontend-only app): server
    // sync and event streaming are skipped and the local mirror is the store.
    let gatewayUnavailable = false;

    /**
     * @param {string} apiPath
     * @param {Omit<RequestInit, 'cache'> & { cache?: FyloCachePolicy }} [init]
     * @returns {Promise<Response>}
     */
    async function fetchAPI(apiPath, init = {}) {
        const url = absoluteUrl(`${browserPath}${apiPath}`);
        // Keep the Fylo cache *policy* out of the RequestInit: 'network-first' /
        // 'cache-first' are not valid `RequestCache` enum values, so leaking them
        // into `new Request(url, { cache })` throws. The policy is passed only as
        // TacBrowserCache.fetch's options argument.
        const { cache: cachePolicy = 'network-first', ...requestInit } = init;
        const headers = new Headers(requestInit.headers);
        if (basicAuth) headers.set('Authorization', `Basic ${basicAuth}`);
        headers.set('Content-Type', 'application/json');

        // TacBrowserCache.fetch() handles IndexedDB read/write, cache policies,
        // and offline fallback transparently.
        const response = await tacBrowserCache.fetch(url, { ...requestInit, headers }, { cache: cachePolicy });
        if (response.status === 404) gatewayUnavailable = true;
        return response;
    }

    // ── Collection facade factory ─────────────────────────────────────
    /**
     * @param {string} collection
     * @returns {Record<string, Function>}
     */
    function createCollectionFacade(collection) {
        /**
         * @param {Record<string, unknown>} request
         * @returns {Promise<any>}
         */
        async function execMachine(request) {
            const response = await fetchAPI('/v1/exec', {
                method: 'POST',
                body: JSON.stringify(request),
                cache: 'no-store',
            });
            const payload = await response.json();
            if (!payload.ok) throw new Error(payload.error?.message || 'FYLO machine request failed');
            return payload.result;
        }

        return {
            /**
             * Find documents with PostgREST-style query parameters.
             * Mirrors the Tachyon fylo-global.js API: returns { docs, collection, encryptedFields }.
             * @param {FyloQuery} [query]
             * @param {FyloQueryOptions} [options]
             * @returns {Promise<FyloFindResult>}
             */
            async find(query = {}, options = {}) {
                const searchParams = new URLSearchParams();
                if (query.limit) searchParams.set('limit', String(query.limit));
                if (query.order) searchParams.set('order', query.order);
                if (query.select) searchParams.set('select', query.select);
                for (const [key, value] of Object.entries(query)) {
                    if (['limit', 'order', 'select', '$ops', '$limit', '$select'].includes(key)) continue;
                    searchParams.set(key, String(value));
                }
                const qs = searchParams.toString();
                const apiPath = `/api/docs?collection=${encodeURIComponent(collection)}${qs ? '&' + qs : ''}`;
                const col = await getLocalCollection(collection);
                try {
                    const response = await fetchAPI(apiPath, { cache: options.cache });
                    if (!response.ok) {
                        // Fall back to local-only query
                        return { docs: await localDocs(col, query), collection };
                    }
                    const payload = await response.json();
                    return { docs: payload.docs || [], collection: payload.collection, encryptedFields: payload.encryptedFields };
                } catch {
                    return { docs: await localDocs(col, query), collection, local: true };
                }
            },

            /**
             * Get a single document by ID.
             * @param {string} id
             * @param {FyloQueryOptions} [options]
             */
            async get(id, options = {}) {
                const col = await getLocalCollection(collection);
                const localGet = async () => {
                    if (!col) return { doc: null };
                    try {
                        const result = await col.get(id).once();
                        return { doc: /** @type {Record<string, unknown>} */ (result)?.[id] || null };
                    } catch {
                        return { doc: null };
                    }
                };
                try {
                    const response = await fetchAPI(`/api/doc?collection=${encodeURIComponent(collection)}&id=${encodeURIComponent(id)}`, { cache: options.cache });
                    if (!response.ok) return await localGet();
                    const payload = await response.json();
                    return { doc: payload.doc, docError: payload.docError };
                } catch {
                    return await localGet();
                }
            },

            /** @param {Record<string, unknown>} doc Create a document. */
            async create(doc) {
                const col = await getLocalCollection(collection);
                const localId = await localWrite(col, (c) => c.put(/** @type {Record<string, any>} */ (doc)));
                // Push to server
                try {
                    const response = await fetchAPI(`/v1/${encodeURIComponent(collection)}`, {
                        method: 'POST',
                        body: JSON.stringify(doc),
                        cache: 'no-store',
                    });
                    // 404 = the /_fylo gateway is not mounted (frontend-only
                    // app); the local mirror write is the operation.
                    if (response.status === 404 && localId) return { ok: true, id: localId, local: true };
                    const payload = await response.json();
                    if (payload.ok) {
                        // Sync the server response back to local
                        if (payload.result?.id) {
                            try { await col?.put({ ...doc, [payload.result.id]: doc }); } catch { /* best effort */ }
                        }
                        return { ok: true, id: payload.result?.id || localId };
                    }
                    return { ok: false, error: payload.error?.message };
                } catch (error) {
                    if (localId) return { ok: true, id: localId };
                    return { ok: false, error: error instanceof Error ? error.message : String(error) };
                }
            },

            /** @param {string} id @param {Record<string, unknown>} doc Patch a document. */
            async patch(id, doc) {
                const col = await getLocalCollection(collection);
                if (col) {
                    try { await col.patch(id, /** @type {Record<string, any>} */ (doc)); } catch { /* local best-effort */ }
                }
                try {
                    const response = await fetchAPI(`/v1/${encodeURIComponent(collection)}/${encodeURIComponent(id)}`, {
                        method: 'PATCH',
                        body: JSON.stringify(doc),
                        cache: 'no-store',
                    });
                    if (response.status === 404 && col) return { ok: true, id, local: true };
                    const payload = await response.json();
                    if (payload.ok) return { ok: true, id: payload.result?.id || id };
                    return { ok: false, error: payload.error?.message };
                } catch (error) {
                    return { ok: false, error: error instanceof Error ? error.message : String(error) };
                }
            },

            /** @param {string} id Delete a document. */
            async del(id) {
                const col = await getLocalCollection(collection);
                if (col) {
                    try { await col.delete(id); } catch { /* local best-effort */ }
                }
                try {
                    const response = await fetchAPI(`/v1/${encodeURIComponent(collection)}/${encodeURIComponent(id)}`, {
                        method: 'DELETE',
                        cache: 'no-store',
                    });
                    if (response.status === 404 && col) return { ok: true, local: true };
                    const payload = await response.json();
                    if (payload.ok) return { ok: true };
                    return { ok: false, error: payload.error?.message };
                } catch (error) {
                    return { ok: false, error: error instanceof Error ? error.message : String(error) };
                }
            },

            /** Alias matching FYLO's language-client collection facade. */
            async delete(/** @type {string} */ id) {
                return this.del(id);
            },

            /** List documents (same as find with no filters). */
            async list(limit = 25, options = {}) {
                return this.find({ limit }, options);
            },

            /** @param {string} id @param {Record<string, unknown>} doc Put/replace a document. */
            async put(id, doc) {
                return this.create({ ...doc, _id: id });
            },

            async createCollection() {
                const col = await getLocalCollection(collection);
                if (col) {
                    try { await col.create(); } catch { /* local best-effort */ }
                }
                try {
                    await execMachine({ op: 'createCollection', collection });
                    return { ok: true };
                } catch (error) {
                    return { ok: Boolean(col), error: col ? undefined : error instanceof Error ? error.message : String(error) };
                }
            },

            async dropCollection() {
                const col = await getLocalCollection(collection);
                if (col) {
                    try { await col.drop(); } catch { /* local best-effort */ }
                }
                try {
                    await execMachine({ op: 'dropCollection', collection });
                    return { ok: true };
                } catch (error) {
                    return { ok: Boolean(col), error: col ? undefined : error instanceof Error ? error.message : String(error) };
                }
            },

            async inspect() {
                const col = await getLocalCollection(collection);
                try {
                    return await execMachine({ op: 'inspectCollection', collection });
                } catch {
                    /* local fallback below */
                }
                return col ? col.inspect() : { exists: false };
            },

            async rebuild() {
                const col = await getLocalCollection(collection);
                try {
                    const response = await fetchAPI('/api/rebuild', {
                        method: 'POST',
                        body: JSON.stringify({ collection }),
                        cache: 'no-store',
                    });
                    if (response.status === 404 && col) return { ok: true, result: await col.rebuild(), local: true };
                    const payload = await response.json();
                    if (payload.ok) return { ok: true, result: payload.result };
                    return { ok: false, error: payload.error };
                } catch (error) {
                    if (col) return { ok: true, result: await col.rebuild() };
                    return { ok: false, error: error instanceof Error ? error.message : String(error) };
                }
            },

            /** @param {Record<string, unknown>[]} docs */
            async batchPut(docs) {
                const col = await getLocalCollection(collection);
                /** @type {unknown[]} */
                const localIds = /** @type {unknown[]} */ (await localWrite(col, (c) => c.batchPut(/** @type {Record<string, any>[]} */ (docs)))) ?? [];
                try {
                    const result = await execMachine({ op: 'batchPutData', collection, batch: docs });
                    return { ok: true, ids: Array.isArray(result) ? result : result?.ids || localIds };
                } catch (error) {
                    return { ok: localIds.length > 0, ids: localIds, error: localIds.length > 0 ? undefined : error instanceof Error ? error.message : String(error) };
                }
            },

            /** @param {Record<string, unknown>} update */
            async patchMany(update) {
                const col = await getLocalCollection(collection);
                if (col) {
                    try { await col.patchMany(/** @type {Record<string, any>} */ (update)); } catch { /* local best-effort */ }
                }
                try {
                    const result = await execMachine({ op: 'patchDocs', collection, update });
                    return { ok: true, result };
                } catch (error) {
                    return { ok: Boolean(col), error: col ? undefined : error instanceof Error ? error.message : String(error) };
                }
            },

            /** @param {Record<string, unknown>} query */
            async deleteMany(query) {
                const col = await getLocalCollection(collection);
                if (col) {
                    try { await col.deleteMany(/** @type {Record<string, any>} */ (query)); } catch { /* local best-effort */ }
                }
                try {
                    const result = await execMachine({ op: 'delDocs', collection, query });
                    return { ok: true, result };
                } catch (error) {
                    return { ok: Boolean(col), error: col ? undefined : error instanceof Error ? error.message : String(error) };
                }
            },

            /** @param {string} id */
            async restore(id) {
                const col = await getLocalCollection(collection);
                if (col) {
                    try { await col.restore(id); } catch { /* local best-effort */ }
                }
                try {
                    const response = await fetchAPI('/api/restore', {
                        method: 'POST',
                        body: JSON.stringify({ collection, id }),
                        cache: 'no-store',
                    });
                    if (response.status === 404 && col) return { ok: true, id, local: true };
                    const payload = await response.json();
                    if (payload.ok) return { ok: true, id: payload.id || id };
                    return { ok: false, error: payload.error };
                } catch (error) {
                    return { ok: Boolean(col), id, error: col ? undefined : error instanceof Error ? error.message : String(error) };
                }
            },

            /** @param {string} id */
            async latest(id) {
                const col = await getLocalCollection(collection);
                try {
                    return { doc: await execMachine({ op: 'getLatest', collection, id }) };
                } catch {
                    return { doc: col ? await col.latest(id) : null };
                }
            },

            /**
             * Subscribe to collection changes. Bridges local subscribe() with server SSE.
             * @param {FyloSubscribeCallback | FyloQuery} callbackOrQuery
             * @param {FyloSubscribeCallback | FyloSubscribeOptions} [optionsOrCallback]
             * @param {FyloSubscribeOptions} [maybeOptions]
             * @returns {() => void}
             */
            subscribe(callbackOrQuery, optionsOrCallback, maybeOptions) {
                /** @type {FyloSubscribeCallback} */
                let callback;
                /** @type {FyloSubscribeOptions} */
                let opts;
                /** @type {Record<string, unknown>} */
                let query = {};

                if (typeof callbackOrQuery === 'function') {
                    callback = callbackOrQuery;
                    opts = /** @type {FyloSubscribeOptions} */ (optionsOrCallback || {});
                } else {
                    query = callbackOrQuery || {};
                    callback = /** @type {FyloSubscribeCallback} */ (optionsOrCallback);
                    opts = maybeOptions || {};
                }

                const pollMs = opts.pollMs || 3000;
                let active = true;
                let offset = opts.since || 0;
                const hasExplicitSince = opts.since !== undefined;
                /** @type {ReturnType<typeof setInterval> | null} */
                let pollTimer = null;
                /** @type {any} */
                let eventSource = null;
                let unsubscribeLocal = () => {};

                // Run initial query and deliver results
                const deliverInitial = async () => {
                    const result = await this.find(query, { cache: opts.cache });
                    callback(result, {
                        collection,
                        events: [],
                        offset: 0,
                        source: 'initial',
                    });
                };

                // Re-run query after events and deliver updated results
                const deliverQueryRefresh = async (/** @type {FyloEventsPayload} */ eventsPayload) => {
                    const result = await this.find(query, { cache: 'no-store' });
                    callback(result, {
                        collection,
                        events: eventsPayload.events || [],
                        offset: eventsPayload.offset || offset,
                        source: 'poll',
                    });
                };

                // Async setup runs in the background so the unsubscribe handle is
                // returned synchronously (callers use it as a function, not a promise).
                (async () => {
                    // Subscribe to local changes. Failures here must not
                    // prevent the initial delivery below.
                    try {
                        const localCol = await getLocalCollection(collection);
                        if (!active) return;
                        if (localCol && typeof localCol.subscribe === 'function') {
                            unsubscribeLocal = localCol.subscribe((/** @type {{ id: string, ts: number, action: string, doc?: Record<string, any> }} */ event) => {
                                if (!active) return;
                                onBrowserEvent({ id: event.id, ts: event.ts, action: event.action, collection, doc: event.doc });
                                this.find(query, { cache: 'no-store' }).then((/** @type {FyloFindResult} */ result) => {
                                    if (!active) return;
                                    callback(result, {
                                        collection,
                                        events: [{ op: event.action, id: event.id }],
                                        offset,
                                        source: 'local',
                                    });
                                });
                            });
                        }
                    } catch (error) {
                        if (opts.onError) opts.onError(error);
                    }

                    await deliverInitial();
                    if (!active) return;

                    // Frontend-only app: the gateway is not mounted, so the
                    // local mirror subscription above is the event source.
                    if (gatewayUnavailable) return;

                    // Try SSE first, fall back to polling
                    const useSSE = typeof EventSource !== 'undefined';
                    if (useSSE) {
                        const sseUrl = `${browserPath}/api/events/stream?collection=${encodeURIComponent(collection)}&since=${offset}&poll=${pollMs}`;
                        eventSource = new EventSource(sseUrl);
                        eventSource.addEventListener('fylo.events', async (/** @type {MessageEvent} */ msg) => {
                            if (!active) return;
                            try {
                                const payload = /** @type {FyloEventsPayload} */ (JSON.parse(msg.data));
                                if (payload.events && payload.events.length > 0) {
                                    offset = payload.offset || offset;
                                    await deliverQueryRefresh(payload);
                                }
                            } catch (error) {
                                if (opts.onError) opts.onError(error);
                            }
                        });
                        eventSource.onerror = () => {
                            if (opts.onError) opts.onError(new Error('FYLO subscription stream interrupted'));
                        };
                    } else {
                        const poll = async () => {
                            if (!active) return;
                            try {
                                const sinceParam = offset === 0 && !hasExplicitSince ? 'latest' : String(offset);
                                const response = await fetchAPI(
                                    `/api/events?collection=${encodeURIComponent(collection)}&since=${sinceParam}&limit=100`,
                                    { cache: 'no-store' },
                                );
                                const payload = /** @type {FyloEventsPayload} */ (await response.json());
                                if (payload.events && payload.events.length > 0) {
                                    offset = payload.offset || offset;
                                    await deliverQueryRefresh(payload);
                                }
                            } catch (error) {
                                if (opts.onError) opts.onError(error);
                            }
                        };
                        poll();
                        pollTimer = setInterval(poll, pollMs);
                    }
                })();

                return () => {
                    active = false;
                    unsubscribeLocal();
                    if (eventSource) eventSource.close();
                    if (pollTimer) clearInterval(pollTimer);
                };
            },
        };
    }

    // ── Root API ──────────────────────────────────────────────────────
    /** @type {Record<string, Record<string, Function>>} */
    const collectionFacades = {};

    /**
     * @returns {Record<string, Record<string, Function>>}
     */
    function createCollectionProxy() {
        return new Proxy(collectionFacades, {
            get(target, prop) {
                if (typeof prop !== 'string' || prop.startsWith('_')) return Reflect.get(target, prop);
                const name = prop;
                if (!target[name]) target[name] = createCollectionFacade(name);
                return target[name];
            },
        });
    }

    const collectionProxy = createCollectionProxy();

    // Cast the dynamic collection proxy to `any` for the merge: its string-index
    // signature can't be expressed alongside FyloApi's named methods in TS.
    /** @type {FyloApi} */
    const fyloApi = Object.assign(/** @type {any} */ (collectionProxy), {
        /** @type {boolean} */
        enabled: false,
        /** @type {string | undefined} */
        root: undefined,
        /**
         * Named collection facade for static-language companions. Dynamic
         * JavaScript can continue to use `fylo.users`; generated companions
         * need an identifier-independent form that survives compilation.
         * @param {string} collection
         * @returns {FyloCollection}
         */
        collection(collection) {
            const name = String(collection);
            if (!collectionFacades[name]) collectionFacades[name] = createCollectionFacade(name);
            return /** @type {FyloCollection} */ (collectionFacades[name]);
        },
        async collections() {
            try {
                const response = await fetchAPI('/api/collections', { cache: 'reload' });
                if (!response.ok) return { root: '', collections: [] };
                return response.json();
            } catch {
                return { root: '', collections: [] };
            }
        },

        async meta() {
            try {
                const response = await fetchAPI('/api/meta', { cache: 'reload' });
                if (!response.ok) return null;
                return response.json();
            } catch {
                return null;
            }
        },

        /** @param {string} user @param {string} pass */
        setCredentials(user, pass) {
            basicAuth = btoa(`${user}:${pass}`);
            const cache = /** @type {{ clearCredentials?: () => void }} */ (tacBrowserCache);
            if (cache.clearCredentials) cache.clearCredentials();
        },

        clearCredentials() {
            basicAuth = null;
            const cache = /** @type {{ clearCredentials?: () => void }} */ (tacBrowserCache);
            if (cache.clearCredentials) cache.clearCredentials();
        },

        /** @param {TemplateStringsArray} strings @param {...unknown} values */
        async sql(strings, ...values) {
            const text = strings.reduce((sql, part, i) => sql + part + (i < values.length ? String(values[i]) : ''), '');
            try {
                const response = await fetchAPI('/v1/sql', {
                    method: 'POST',
                    body: JSON.stringify({ sql: text }),
                    cache: 'no-store',
                });
                const payload = await response.json();
                if (payload.ok) return payload.result;
                throw new Error(payload.error?.message || 'SQL query failed');
            } catch (error) {
                // Fall back to local SQL; if there's no local client, surface
                // the original server error.
                const client = await getLocalClient();
                if (!client) throw error;
                return client.sql(strings, ...values);
            }
        },

        /** @param {string} apiPath @param {Omit<RequestInit, 'cache'> & { cache?: FyloCachePolicy }} [init] */
        async request(apiPath, init = {}) {
            return fetchAPI(apiPath, init);
        },

        /** @param {string} collection */
        async createCollection(collection) {
            return createCollectionFacade(collection).createCollection();
        },

        /** @param {string} collection */
        async dropCollection(collection) {
            return createCollectionFacade(collection).dropCollection();
        },

        /** @param {string} collection */
        async inspectCollection(collection) {
            return createCollectionFacade(collection).inspect();
        },

        /** @param {string} collection */
        async rebuildCollection(collection) {
            return createCollectionFacade(collection).rebuild();
        },

        /** @param {(event: { id: string, ts: number, action: string, collection: string, doc?: Record<string, any> }) => void} handler */
        onBrowserEvent(handler) {
            onBrowserEvent = handler;
        },
    });

    return {
        proxy: fyloApi,
        async probe() {
            try {
                const meta = await fyloApi.meta();
                if (meta) {
                    fyloApi.enabled = true;
                    fyloApi.root = meta.root;
                }
            } catch {
                /* fylo browser not mounted; leave disabled */
            }
        },
    };
}

/**
 * Self-bootstrapping proxy that lazily creates the client on first access.
 * Companion scripts import `fylo` directly; the compiler injects
 * `import { fylo } from '../runtime/fylo-global.js'`.
 *
 * Resolves `<meta name="fylo-browser-path">` from the document shell (injected
 * by Compiler.renderShellHTML / Yon.renderShellHTML).
 *
 * @type {FyloApi & { _ready: Promise<FyloApi> }}
 */
/** @type {ReturnType<typeof createFyloClient> | null} */
let sharedClient = null;
/** @returns {FyloApi} The single lazily-created client proxy. */
function sharedFyloProxy() {
    if (!sharedClient) sharedClient = createFyloClient();
    return sharedClient.proxy;
}

export const fylo = new Proxy(/** @type {any} */ ({}), {
    get(_target, prop) {
        // Delegate every access to ONE shared client so stateful operations
        // (setCredentials, cached collections) persist across calls. Creating a
        // fresh client per access would drop credentials set on a prior call.
        if (prop === '_ready') return Promise.resolve(sharedFyloProxy());
        return Reflect.get(sharedFyloProxy(), prop);
    },
});

// Documented contract (see src/types/globals.d.ts): `fylo` is also available
// on `window` for plain script tags and devtools.
if (typeof window !== 'undefined') {
    /** @type {any} */ (window).fylo ??= fylo;
}
