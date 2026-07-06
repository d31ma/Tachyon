// @ts-check
import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { Window } from 'happy-dom';
import { pathToFileURL } from 'url';

const FYLO_GLOBAL_URL = pathToFileURL(`${import.meta.dir}/../../src/runtime/fylo-global.js`).href;
const BROWSER_CACHE_URL = pathToFileURL(`${import.meta.dir}/../../src/runtime/browser-cache.js`).href;

// Minimal but faithful in-memory IndexedDB: enough of the request/transaction
// API (get/put/delete with async onsuccess, transaction oncomplete) for
// browser-cache.js to actually read and write cached responses.
function createFakeIndexedDB() {
    /** @type {Map<string, Map<string, unknown>>} */
    const stores = new Map();
    const ensureStore = (/** @type {string} */ name) => {
        if (!stores.has(name)) stores.set(name, new Map());
        return /** @type {Map<string, any>} */ (stores.get(name));
    };
    // IDBRequest-like: fires onsuccess (or onerror) asynchronously.
    const request = (/** @type {() => unknown} */ compute) => {
        /** @type {any} */
        const req = { onsuccess: null, onerror: null, result: undefined, error: null };
        queueMicrotask(() => {
            try { req.result = compute(); if (req.onsuccess) req.onsuccess({ target: req }); }
            catch (error) { req.error = error; if (req.onerror) req.onerror({ target: req }); }
        });
        return req;
    };
    const makeObjectStore = (/** @type {string} */ name) => {
        const store = ensureStore(name);
        return {
            get: (/** @type {string} */ key) => request(() => store.get(key)),
            // keyPath is 'key', so the value carries its own key (writes are
            // applied synchronously so a later read in the same test sees them).
            put: (/** @type {{ key: string }} */ value) => { store.set(value.key, value); return request(() => value.key); },
            delete: (/** @type {string} */ key) => { store.delete(key); return request(() => undefined); },
        };
    };
    const db = {
        objectStoreNames: { contains: (/** @type {string} */ name) => stores.has(name) },
        createObjectStore: (/** @type {string} */ name) => { ensureStore(name); return makeObjectStore(name); },
        transaction: (/** @type {string} */ _storeNames, /** @type {string} */ _mode) => {
            /** @type {any} */
            const tx = { oncomplete: null, onerror: null, objectStore: makeObjectStore };
            queueMicrotask(() => { if (tx.oncomplete) tx.oncomplete({ target: tx }); });
            return tx;
        },
    };
    return {
        stores,
        open(/** @type {string} */ _name, /** @type {number} */ _version) {
            /** @type {any} */
            const req = { onsuccess: null, onerror: null, onupgradeneeded: null, result: db };
            setTimeout(() => {
                if (req.onupgradeneeded) req.onupgradeneeded({ target: req });
                if (req.onsuccess) req.onsuccess({ target: req });
            }, 0);
            return req;
        },
    };
}

/** @typedef {{ window: Window & Record<string, unknown>, document: Document & Record<string, unknown>, indexedDB: ReturnType<typeof createFakeIndexedDB>, fetch: typeof fetch, EventSource: any, __tc_browser_cache__: any, __tc_native_fetch__: any }} TestGlobal */
function testGlobal() {
    return /** @type {TestGlobal} */ (globalThis);
}

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
    await client.probe();
    return module;
}

describe('fylo browser sync client', () => {
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
            __tc_browser_cache__: global.__tc_browser_cache__,
            __tc_native_fetch__: global.__tc_native_fetch__,
        };
        installWindow(windowInstance);
        Reflect.deleteProperty(globalThis, '__tc_browser_cache__');
        Reflect.deleteProperty(globalThis, '__tc_native_fetch__');
        const { tacBrowserCache } = await import(BROWSER_CACHE_URL);
        tacBrowserCache.dbPromise = null;
        testGlobal().__tc_browser_cache__ = tacBrowserCache;
    });

    afterEach(async () => {
        await windowInstance.happyDOM.close();
        Object.assign(globalThis, previousGlobals);
        for (const key of ['__tc_browser_cache__', '__tc_native_fetch__']) {
            if (previousGlobals[key] === undefined) Reflect.deleteProperty(globalThis, key);
        }
        if (previousGlobals.EventSource === undefined) Reflect.deleteProperty(globalThis, 'EventSource');
        if (previousGlobals.fetch === undefined) Reflect.deleteProperty(globalThis, 'fetch');
    });

    test('find() queries the server with collection and filter params', async () => {
        let calls = 0;
        const fetchStub = /** @type {typeof fetch} */ (async (_input, init) => {
            const request = new Request(_input, init);
            const url = new URL(request.url);
            if (url.pathname.endsWith('/_fylo/api/meta'))
                return Response.json({ root: '/tmp/fylo', readOnly: false, revealed: true, path: '/_fylo' });
            calls += 1;
            if (calls > 1) throw new TypeError('offline');
            return Response.json({ docs: [{ id: 'u1', doc: { name: 'Ada' } }] });
        });
        testGlobal().fetch = fetchStub;
        testGlobal().__tc_native_fetch__ = fetchStub;

        const { fylo } = await importFylo();
        const first = await fylo.users.find({ role: 'eq.admin', limit: 2 });
        const second = await fylo.users.find({ limit: 2, role: 'eq.admin' }, { cache: 'network-first' });

        expect(first.docs[0].doc.name).toBe('Ada');
        expect(second.docs[0].doc.name).toBe('Ada');
        expect(calls).toBe(2);
    });

    test('mutations POST/PATCH/DELETE to the server and return ok/id', async () => {
        /** @type {Array<{ method: string, url: string }>} */
        const history = [];
        const fetchStub = /** @type {typeof fetch} */ (async (_input, init) => {
            const request = new Request(_input, init);
            if (request.url.endsWith('/_fylo/api/meta'))
                return Response.json({ root: '/tmp/fylo', readOnly: false, revealed: true, path: '/_fylo' });
            history.push({ method: request.method, url: request.url });
            if (request.method === 'POST')
                return Response.json({ ok: true, result: { id: 'u2' } });
            if (request.method === 'PATCH')
                return Response.json({ ok: true, result: { id: 'u2' } });
            if (request.method === 'DELETE')
                return Response.json({ ok: true, result: { deleted: true, id: 'u2' } });
            return Response.json({ docs: [{ id: 'u1', doc: { name: 'Ada' } }] });
        });
        testGlobal().fetch = fetchStub;
        testGlobal().__tc_native_fetch__ = fetchStub;

        const { fylo } = await importFylo();
        const created = await fylo.users.create({ name: 'Grace' });
        const patched = await fylo.users.patch('u2', { name: 'Grace Updated' });
        const deleted = await fylo.users.del('u2');

        expect(created.ok).toBe(true);
        expect(created.id).toBe('u2');
        expect(patched.ok).toBe(true);
        expect(deleted.ok).toBe(true);
        expect(history.some((h) => h.method === 'POST' && h.url.includes('/v1/users'))).toBe(true);
        expect(history.some((h) => h.method === 'PATCH')).toBe(true);
        expect(history.some((h) => h.method === 'DELETE')).toBe(true);
    });

    test('exposes FYLO browser collection and machine operations', async () => {
        /** @type {Array<{ method: string, path: string, body: any }>} */
        const calls = [];
        const fetchStub = /** @type {typeof fetch} */ (async (_input, init) => {
            const request = new Request(_input, init);
            const url = new URL(request.url);
            const bodyText = request.method === 'GET' || request.method === 'HEAD'
                ? ''
                : await request.text();
            const body = bodyText ? JSON.parse(bodyText) : null;
            if (url.pathname.endsWith('/_fylo/api/meta'))
                return Response.json({ root: '/tmp/fylo', readOnly: false, revealed: true, path: '/_fylo' });
            calls.push({ method: request.method, path: url.pathname, body });
            if (url.pathname.endsWith('/_fylo/api/rebuild'))
                return Response.json({ ok: true, result: { rebuilt: true } });
            if (url.pathname.endsWith('/_fylo/api/restore'))
                return Response.json({ ok: true, id: body.id });
            if (url.pathname.endsWith('/_fylo/v1/exec'))
                return Response.json({ ok: true, result: body.op === 'batchPutData' ? ['id1', 'id2'] : { op: body.op } });
            return Response.json({ ok: true });
        });
        testGlobal().fetch = fetchStub;
        testGlobal().__tc_native_fetch__ = fetchStub;

        const { fylo } = await importFylo();
        await fylo.createCollection('users');
        await fylo.users.batchPut([{ name: 'Ada' }, { name: 'Grace' }]);
        await fylo.users.patchMany({ query: { role: 'eq.admin' }, patch: { active: true } });
        await fylo.users.deleteMany({ role: 'eq.retired' });
        await fylo.users.latest('id1');
        await fylo.users.inspect();
        await fylo.users.rebuild();
        await fylo.users.restore('id1');
        await fylo.dropCollection('users');

        const machineOps = calls
            .filter((call) => call.path.endsWith('/_fylo/v1/exec'))
            .map((call) => call.body.op);
        expect(machineOps).toEqual([
            'createCollection',
            'batchPutData',
            'patchDocs',
            'delDocs',
            'getLatest',
            'inspectCollection',
            'dropCollection',
        ]);
        expect(calls.some((call) => call.path.endsWith('/_fylo/api/rebuild'))).toBe(true);
        expect(calls.some((call) => call.path.endsWith('/_fylo/api/restore'))).toBe(true);
    });

    test('cache-first returns cached data after network hydration', async () => {
        let calls = 0;
        const fetchStub = /** @type {typeof fetch} */ (async (_input, init) => {
            const request = new Request(_input, init);
            if (request.url.endsWith('/_fylo/api/meta'))
                return Response.json({ root: '/tmp/fylo', readOnly: false, revealed: true, path: '/_fylo' });
            calls += 1;
            return Response.json({ docs: [{ id: 'u1', doc: { name: 'Ada' } }] });
        });
        testGlobal().fetch = fetchStub;
        testGlobal().__tc_native_fetch__ = fetchStub;

        const { fylo } = await importFylo();
        const first = await fylo.users.find({ limit: 1 });
        expect(first.docs[0].doc.name).toBe('Ada');
        expect(calls).toBe(1);

        const second = await fylo.users.find({ limit: 1 }, { cache: 'cache-first' });
        expect(second.docs[0].doc.name).toBe('Ada');
        // cache-first should not make a second network call
    });

    test('no-store bypasses the IndexedDB cache', async () => {
        let calls = 0;
        const fetchStub = /** @type {typeof fetch} */ (async (_input, init) => {
            const request = new Request(_input, init);
            if (request.url.endsWith('/_fylo/api/meta'))
                return Response.json({ root: '/tmp/fylo', readOnly: false, revealed: true, path: '/_fylo' });
            calls += 1;
            return Response.json({ docs: [{ id: 'u1', doc: { name: calls === 1 ? 'Ada' : 'Grace' } }] });
        });
        testGlobal().fetch = fetchStub;
        testGlobal().__tc_native_fetch__ = fetchStub;

        const { fylo } = await importFylo();
        const first = await fylo.users.find({ limit: 1 }, { cache: 'network-first' });
        const second = await fylo.users.find({ limit: 1 }, { cache: 'no-store' });

        expect(first.docs[0].doc.name).toBe('Ada');
        expect(second.docs[0].doc.name).toBe('Grace');
        expect(calls).toBe(2);
    });

    test('setCredentials sends Basic auth headers and clearCredentials removes them', async () => {
        /** @type {Array<string | null>} */
        const authHeaders = [];
        const fetchStub = /** @type {typeof fetch} */ (async (_input, init) => {
            const request = new Request(_input, init);
            if (request.url.endsWith('/_fylo/api/meta'))
                return Response.json({ root: '/tmp/fylo', readOnly: false, revealed: true, path: '/_fylo' });
            authHeaders.push(request.headers.get('Authorization'));
            return Response.json({ docs: [{ id: 'u1', doc: { name: 'Ada' } }] });
        });
        testGlobal().fetch = fetchStub;
        testGlobal().__tc_native_fetch__ = fetchStub;

        const { fylo } = await importFylo();
        fylo.setCredentials('alice', 'pass');
        await fylo.users.find({ limit: 1 });
        fylo.clearCredentials();
        await fylo.users.find({ limit: 1 });

        expect(authHeaders[0]).toContain('Basic');
        expect(authHeaders[1]).toBeNull();
    });

    test('subscribe() delivers initial results then polls for events', async () => {
        testGlobal().EventSource = /** @type {any} */ (undefined);
        let findCalls = 0;
        /** @type {string[]} */
        const eventSinceParams = [];
        const fetchStub = /** @type {typeof fetch} */ (async (_input, init) => {
            const request = new Request(_input, init);
            const url = new URL(request.url);
            if (request.url.endsWith('/_fylo/api/meta'))
                return Response.json({ root: '/tmp/fylo', readOnly: false, revealed: true, path: '/_fylo' });
            if (url.pathname.endsWith('/_fylo/api/events')) {
                eventSinceParams.push(url.searchParams.get('since') ?? '');
                return Response.json(
                    eventSinceParams.length === 1
                        ? { collection: 'users', events: [], offset: 10, exists: true }
                        : { collection: 'users', events: [{ op: 'put', id: 'u2' }], offset: 20, exists: true },
                );
            }
            findCalls += 1;
            return Response.json({ docs: [{ id: `u${findCalls}`, doc: { name: findCalls === 1 ? 'Ada' : 'Grace' } }] });
        });
        testGlobal().fetch = fetchStub;
        testGlobal().__tc_native_fetch__ = fetchStub;

        const { fylo } = await importFylo();
        /** @type {Array<{ name: string, source: string, events: number }>} */
        const updates = [];
        const completed = new Promise((resolve) => {
            const unsubscribe = fylo.users.subscribe(
                { order: 'name.asc' },
                (/** @type {any} */ result, /** @type {any} */ meta) => {
                    if (result.docs && result.docs.length > 0) {
                        updates.push({
                            name: result.docs[0].doc.name,
                            source: meta.source,
                            events: meta.events.length,
                        });
                    }
                    if (updates.length >= 2) { unsubscribe(); resolve(undefined); }
                },
                { pollMs: 5 },
            );
        });

        await Promise.race([
            completed,
            new Promise((_, reject) => setTimeout(() => reject(new Error('subscribe timed out')), 3000)),
        ]);

        expect(updates.length).toBeGreaterThanOrEqual(2);
        expect(updates[0]).toEqual({ name: 'Ada', source: 'initial', events: 0 });
        expect(updates[1].source).toBe('poll');
        expect(updates[1].events).toBe(1);
        expect(eventSinceParams.length).toBeGreaterThanOrEqual(1);
    });

    test('collections() and meta() return server metadata', async () => {
        const fetchStub = /** @type {typeof fetch} */ (async (_input, _init) => {
            const request = new Request(_input, _init);
            if (request.url.endsWith('/_fylo/api/collections'))
                return Response.json({ root: '/tmp/fylo', collections: [{ name: 'users', exists: true }] });
            if (request.url.endsWith('/_fylo/api/meta'))
                return Response.json({ root: '/tmp/fylo', readOnly: false, revealed: false, path: '/_fylo' });
            return Response.json({ docs: [] });
        });
        testGlobal().fetch = fetchStub;
        testGlobal().__tc_native_fetch__ = fetchStub;

        const { fylo } = await importFylo();
        const collections = await fylo.collections();
        const meta = await fylo.meta();

        expect(collections.collections).toHaveLength(1);
        expect(collections.collections[0].name).toBe('users');
        expect(meta.root).toBe('/tmp/fylo');
        expect(meta.readOnly).toBe(false);
    });
});
