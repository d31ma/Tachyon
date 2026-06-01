// @ts-check
import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { Window } from 'happy-dom';
import { pathToFileURL } from 'url';

const FYLO_GLOBAL_URL = pathToFileURL(`${import.meta.dir}/../../src/runtime/fylo-global.js`).href;
const BROWSER_CACHE_URL = pathToFileURL(`${import.meta.dir}/../../src/runtime/browser-cache.js`).href;

function createFakeIndexedDB() {
    const stores = new Map();
    /** @param {string} name */
    const ensureStore = (name) => {
        if (!stores.has(name))
            stores.set(name, new Map());
        return stores.get(name);
    };
    const api = {
        stores,
        /** @param {string} _name @param {number} _version */
        open(_name, _version) {
            /** @type {any} */
            const request = {
                result: {
                    objectStoreNames: {
                        /** @param {string} storeName */
                        contains(storeName) {
                            return stores.has(storeName);
                        },
                    },
                    /** @param {string} storeName */
                    createObjectStore(storeName) {
                        ensureStore(storeName);
                        return {};
                    },
                    /** @param {string} storeName */
                    transaction(storeName) {
                        const store = ensureStore(storeName);
                        /** @type {any} */
                        const tx = {
                            oncomplete: null,
                            onerror: null,
                            objectStore() {
                                return {
                                    /** @param {string} key */
                                    get(key) {
                                        /** @type {any} */
                                        const getRequest = { result: undefined, onsuccess: null, onerror: null };
                                        queueMicrotask(() => {
                                            getRequest.result = store.get(key);
                                            getRequest.onsuccess?.();
                                        });
                                        return getRequest;
                                    },
                                    /** @param {{ key: string }} value */
                                    put(value) {
                                        store.set(value.key, value);
                                        queueMicrotask(() => tx.oncomplete?.());
                                    },
                                    /** @param {string} key */
                                    delete(key) {
                                        store.delete(key);
                                        queueMicrotask(() => tx.oncomplete?.());
                                    },
                                    openCursor() {
                                        const entries = [...store.keys()];
                                        let index = 0;
                                        /** @type {any} */
                                        const cursorRequest = { result: null, onsuccess: null, onerror: null };
                                        const advance = () => {
                                            const key = entries[index++];
                                            if (key === undefined) {
                                                cursorRequest.result = null;
                                                cursorRequest.onsuccess?.();
                                                queueMicrotask(() => tx.oncomplete?.());
                                                return;
                                            }
                                            cursorRequest.result = {
                                                key,
                                                delete() { store.delete(key); },
                                                continue() { queueMicrotask(advance); },
                                            };
                                            cursorRequest.onsuccess?.();
                                        };
                                        queueMicrotask(advance);
                                        return cursorRequest;
                                    },
                                };
                            },
                        };
                        return tx;
                    },
                },
                onupgradeneeded: null,
                onsuccess: null,
                onerror: null,
                onblocked: null,
            };
            queueMicrotask(() => {
                request.onupgradeneeded?.();
                request.onsuccess?.();
            });
            return request;
        },
    };
    return api;
}

/**
 * @typedef {typeof globalThis & {
 *   window: Window & { fylo?: unknown },
 *   document: Document,
 *   indexedDB: ReturnType<typeof createFakeIndexedDB>,
 *   EventSource?: typeof EventSource,
 *   fetch: typeof fetch,
 *   __ty_browser_cache__?: any,
 *   __ty_native_fetch__?: typeof fetch
 * }} TestGlobal
 */

/** @returns {TestGlobal} */
function testGlobal() {
    return /** @type {TestGlobal} */ (globalThis);
}

/** @param {Window} windowInstance */
function installWindow(windowInstance) {
    Object.assign(globalThis, {
        window: windowInstance,
        document: windowInstance.document,
        indexedDB: createFakeIndexedDB(),
    });
    windowInstance.document.head.innerHTML = '<meta name="fylo-browser-path" content="/_fylo">';
}

async function importFylo() {
    const module = await import(FYLO_GLOBAL_URL);
    const client = module.createFyloClient('/_fylo');
    testGlobal().window.fylo = client.proxy;
    client.probe();
    return module;
}

describe('fylo browser global cache', () => {
    /** @type {Window} */
    let windowInstance;
    /** @type {Record<string, unknown>} */
    let previousGlobals;

    beforeEach(async () => {
        windowInstance = new Window();
        const global = testGlobal();
        previousGlobals = {
            window: global.window,
            document: global.document,
            indexedDB: global.indexedDB,
            EventSource: global.EventSource,
            fetch: global.fetch,
            __ty_browser_cache__: global.__ty_browser_cache__,
            __ty_native_fetch__: global.__ty_native_fetch__,
        };
        installWindow(windowInstance);
        Reflect.deleteProperty(globalThis, '__ty_browser_cache__');
        Reflect.deleteProperty(globalThis, '__ty_native_fetch__');
        const { tacBrowserCache } = await import(BROWSER_CACHE_URL);
        tacBrowserCache.dbPromise = null;
        testGlobal().__ty_browser_cache__ = tacBrowserCache;
    });

    afterEach(async () => {
        await windowInstance.happyDOM.close();
        Object.assign(globalThis, previousGlobals);
        for (const key of ['__ty_browser_cache__', '__ty_native_fetch__']) {
            if (previousGlobals[key] === undefined)
                Reflect.deleteProperty(globalThis, key);
        }
        if (previousGlobals.EventSource === undefined)
            Reflect.deleteProperty(globalThis, 'EventSource');
    });

    test('find() uses canonical query keys and falls back to IndexedDB when offline', async () => {
        let calls = 0;
        const fetchStub = /** @type {typeof fetch} */ (async (_input, init) => {
            const request = new Request(_input, init);
            if (request.url.endsWith('/_fylo/api/meta'))
                return Response.json({ root: '/tmp/fylo', readOnly: false, revealed: true, path: '/_fylo' });
            calls += 1;
            if (calls > 1)
                throw new TypeError('offline');
            return Response.json({ docs: [{ id: 'u1', doc: { name: 'Ada' } }] });
        });
        testGlobal().fetch = fetchStub;
        testGlobal().__ty_native_fetch__ = fetchStub;

        const { fylo } = await importFylo();
        const first = await fylo.users.find({ role: 'eq.admin', limit: 2 });
        const second = await fylo.users.find({ limit: 2, role: 'eq.admin' }, { cache: 'network-first' });

        expect(first.docs[0].doc.name).toBe('Ada');
        expect(second.docs[0].doc.name).toBe('Ada');
        expect(calls).toBe(2);
    });

    test('collection mutations invalidate cached collection queries', async () => {
        let version = 0;
        const fetchStub = /** @type {typeof fetch} */ (async (_input, init) => {
            const request = new Request(_input, init);
            if (request.url.endsWith('/_fylo/api/meta'))
                return Response.json({ root: '/tmp/fylo', readOnly: false, revealed: true, path: '/_fylo' });
            if (request.method === 'POST')
                return Response.json({ ok: true, id: 'u2' });
            version += 1;
            return Response.json({ docs: [{ id: `u${version}`, doc: { version } }] });
        });
        testGlobal().fetch = fetchStub;
        testGlobal().__ty_native_fetch__ = fetchStub;

        const { fylo } = await importFylo();
        const first = await fylo.users.find({ limit: 1 });
        await fylo.users.create({ name: 'Grace' });
        const second = await fylo.users.find({ limit: 1 });

        expect(first.docs[0].doc.version).toBe(1);
        expect(second.docs[0].doc.version).toBe(2);
    });

    test('authenticated FYLO caches are scoped without storing raw credentials in keys', async () => {
        /** @type {Array<string | null>} */
        const seen = [];
        const fetchStub = /** @type {typeof fetch} */ (async (_input, init) => {
            const request = new Request(_input, init);
            if (request.url.endsWith('/_fylo/api/meta'))
                return Response.json({ root: '/tmp/fylo', readOnly: false, revealed: true, path: '/_fylo' });
            seen.push(request.headers.get('Authorization'));
            return Response.json({ docs: [{ id: 'profile', doc: { scope: seen.length } }] });
        });
        testGlobal().fetch = fetchStub;
        testGlobal().__ty_native_fetch__ = fetchStub;

        const { fylo } = await importFylo();
        fylo.setCredentials('alice', 'one');
        const alice = await fylo.users.find({ limit: 1 });
        fylo.setCredentials('bob', 'two');
        const bob = await fylo.users.find({ limit: 1 });
        fylo.setCredentials('alice', 'one');
        const aliceAgain = await fylo.users.find({ limit: 1 });

        const responseStore = testGlobal().indexedDB.stores.get('responses');
        const keys = [...responseStore.keys()].join('\n');

        expect(alice.docs[0].doc.scope).toBe(1);
        expect(bob.docs[0].doc.scope).toBe(2);
        expect(aliceAgain.docs[0].doc.scope).toBe(1);
        expect(seen).toHaveLength(2);
        expect(keys).not.toContain('alice');
        expect(keys).not.toContain('one');
        expect(keys).not.toContain('bob');
        expect(keys).not.toContain('two');
    });

    test('subscribe() refreshes cached queries after FYLO events using the polling fallback', async () => {
        testGlobal().EventSource = /** @type {any} */ (undefined);
        let findCalls = 0;
        let eventCalls = 0;
        /** @type {string[]} */
        const eventOffsets = [];
        const fetchStub = /** @type {typeof fetch} */ (async (_input, init) => {
            const request = new Request(_input, init);
            const url = new URL(request.url);
            if (request.url.endsWith('/_fylo/api/meta'))
                return Response.json({ root: '/tmp/fylo', readOnly: false, revealed: true, path: '/_fylo' });
            if (url.pathname.endsWith('/_fylo/api/events')) {
                eventCalls += 1;
                eventOffsets.push(url.searchParams.get('since') ?? '');
                return Response.json(eventCalls === 1
                    ? { collection: 'users', events: [], offset: 10, exists: true }
                    : { collection: 'users', events: [{ op: 'put', id: 'u2' }], offset: 20, exists: true });
            }
            findCalls += 1;
            return Response.json({
                docs: [{
                    id: `u${findCalls}`,
                    doc: { name: findCalls === 1 ? 'Ada' : 'Grace' },
                }],
            });
        });
        testGlobal().fetch = fetchStub;
        testGlobal().__ty_native_fetch__ = fetchStub;

        const { fylo } = await importFylo();
        /** @type {Array<{ name: string, source: string, events: number }>} */
        const updates = [];
        /** @type {() => void} */
        let unsubscribe = () => {};
        const completed = new Promise((resolve) => {
            unsubscribe = fylo.users.subscribe(
                { order: 'name.asc' },
                (/** @type {any} */ result, /** @type {any} */ meta) => {
                    updates.push({
                        name: result.docs[0].doc.name,
                        source: meta.source,
                        events: meta.events.length,
                    });
                    if (updates.length === 2) resolve(undefined);
                },
                { pollMs: 5 },
            );
        });

        await Promise.race([
            completed,
            new Promise((_, reject) => setTimeout(() => reject(new Error('subscribe timed out')), 1500)),
        ]);
        unsubscribe();

        expect(updates).toEqual([
            { name: 'Ada', source: 'initial', events: 0 },
            { name: 'Grace', source: 'poll', events: 1 },
        ]);
        expect(eventOffsets).toEqual(['latest', '10']);
        expect(findCalls).toBe(2);
    });
});
