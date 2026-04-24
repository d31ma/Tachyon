// @ts-check
import { afterAll, beforeAll, describe, expect, test } from 'bun:test';
import { Window } from 'happy-dom';
import path from 'path';
import os from 'os';
import { fileURLToPath } from 'url';

/**
 * @typedef {Record<string, any>} TestResults
 * @typedef {(elemId?: string | null, event?: unknown, compId?: string | null) => Promise<string>} RenderResult
 * @typedef {(props?: unknown) => Promise<RenderResult>} TestFactory
 */

const TEMPLATE_PATH = fileURLToPath(new URL('../../src/compiler/render-template.js', import.meta.url));
/**
 * Builds a factory from render-template.js with test code injected into the
 * script slot. Results are returned via globalThis.__ty_test__ to bridge
 * the ESM module boundary.
 */
/**
 * @param {string} testScript
 * @param {string} [testInners]
 * @returns {Promise<TestFactory>}
 */
async function buildTestFactory(testScript, testInners) {
    const source = await Bun.file(TEMPLATE_PATH).text();
    const factorySource = `
const __ty_props__ = __ty_helpers__.decodeProps(props);
const __ty_scope__ = __ty_helpers__.createScope(null, __ty_props__);

with (__ty_scope__) {
    const emit = __ty_helpers__.emit;
    const fetch = __ty_helpers__.fetch;
    const isBrowser = __ty_helpers__.isBrowser;
    const isServer = __ty_helpers__.isServer;
    const onMount = __ty_helpers__.onMount;
    const rerender = __ty_helpers__.rerender;
    const inject = __ty_helpers__.inject;
    const provide = __ty_helpers__.provide;

    ${testScript}

    if (__ty_props__) {
        for (const __k__ of Object.keys(__ty_props__)) {
            if (/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(__k__) && !__k__.startsWith('__ty_')) {
                const __v__ = __ty_props__[__k__];
                try { eval(\`\${__k__} = __v__\`) } catch {}
            }
        }
    }

    const compRenders = new Map();

    return async function(elemId, event, compId) {
        const counters = { id: {}, ev: {}, bind: {} };
        const ty_componentRootId = compId
            ? (String(compId).startsWith('ty-') ? String(compId) : 'ty-' + compId + '-0')
            : null;

        __ty_helpers__.setRenderContext({ componentRootId: ty_componentRootId, elemId, event });

        const ty_generateId = (hash, source) => {
            const key = compId ? hash + '-' + compId : hash;
            const map = counters[source];

            if (key in map) return 'ty-' + key + '-' + map[key]++;

            map[key] = 1;
            return 'ty-' + key + '-0';
        };

        const ty_invokeEvent = async (hash, action) => {
            if (elemId === ty_generateId(hash, 'ev')) {
                if (typeof action === 'function') await action(event);
                else await eval(action);
            }
            return '';
        };

        const ty_assignValue = (hash, variable) => {
            if (elemId === ty_generateId(hash, 'bind') && event) {
                if (/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(variable)) {
                    const __val__ = event.value;
                    eval(\`\${variable} = __val__\`);
                }
            }
            return '';
        };

        const ty_escapeHtml = (value) => {
            if (value === null || value === undefined) return '';
            return String(value)
                .replaceAll('&', '&amp;')
                .replaceAll('<', '&lt;')
                .replaceAll('>', '&gt;')
                .replaceAll('"', '&quot;')
                .replaceAll("'", '&#39;');
        };

        const ty_escapeText = ty_escapeHtml;
        const ty_escapeAttr = ty_escapeHtml;

        let elements = '';
        let render;

        ${testInners ?? ''}

        return elements;
    };
}`;
    const modified = source
        .replace('// module_imports', '')
        .replace('"__TY_FACTORY_SOURCE__"', JSON.stringify(factorySource));
    const tmpPath = path.join(os.tmpdir(), `tachyon-tpl-${Bun.randomUUIDv7()}.js`);
    await Bun.write(tmpPath, modified);
    const { default: factory } = await import(tmpPath);
    return factory;
}

/** @returns {TestResults} */
function testResults() {
    /** @type {TestResults} */
    const results = {};
    /** @type {any} */ (globalThis).__ty_test__ = results;
    return results;
}

function createFakeIndexedDB() {
    const stores = new Map();
    /**
     * @param {string} name
     * @returns {Map<string, any>}
     */
    const ensureStore = (name) => {
        if (!stores.has(name))
            stores.set(name, new Map());
        return stores.get(name);
    };
    return {
        /**
         * @param {string} _name
         * @param {number} _version
         */
        open(_name, _version) {
            /** @type {any} */
            const request = {
                result: {
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
                                        const getRequest = {
                                            result: undefined,
                                            onsuccess: null,
                                            onerror: null,
                                        };
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
                                };
                            },
                        };
                        return tx;
                    },
                },
                onupgradeneeded: null,
                onsuccess: null,
                onerror: null,
            };
            queueMicrotask(() => {
                request.onupgradeneeded?.();
                request.onsuccess?.();
            });
            return request;
        },
    };
}
// ── Server-side (no window) ────────────────────────────────────────────────────
describe('render-template server-side (no window)', () => {
    test('isBrowser is false and isServer is true', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`__ty_test__.isBrowser = isBrowser; __ty_test__.isServer = isServer`);
        await factory();
        expect(r.isBrowser).toBe(false);
        expect(r.isServer).toBe(true);
    });
    test('onMount is a no-op — callback is never registered or called', async () => {
        const r = testResults();
        r.called = false;
        const factory = await buildTestFactory(`onMount(() => { __ty_test__.called = true })`);
        await factory();
        expect(r.called).toBe(false);
    });
    test('inject returns fallback when window is absent', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`__ty_test__.value = inject('key', 'fallback')`);
        await factory();
        expect(r.value).toBe('fallback');
    });
    test('inject returns undefined fallback by default', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`__ty_test__.value = inject('key')`);
        await factory();
        expect(r.value).toBeUndefined();
    });
    test('env returns fallback when window is absent', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`
            const helpers = __ty_helpers__.createTacHelpers({})
            __ty_test__.value = helpers.env('API_BASE_URL', '/fallback')
        `);
        await factory();
        expect(r.value).toBe('/fallback');
    });
    test('persistent $ fields keep their default value without window', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $draft: 42 }
            const helpers = __ty_helpers__.createTacHelpers({})
            helpers.bindPersistentFields(controller)
            controller.$draft = 99
            __ty_test__.val = controller.$draft
        `);
        await factory();
        expect(r.val).toBe(99);
    });
    test('rerender is a no-op without window', async () => {
        const factory = await buildTestFactory(`rerender()`);
        expect(async () => await factory()).not.toThrow();
    });
});
// ── Browser-side (with happy-dom window) ──────────────────────────────────────
describe('render-template browser-side (with window)', () => {
    /** @type {Window} */
    let windowInstance;
    /** @type {Record<string, unknown>} */
    let previousGlobals;
    beforeAll(() => {
        windowInstance = new Window();
        previousGlobals = {
            window: globalThis.window,
            sessionStorage: globalThis.sessionStorage,
            CustomEvent: globalThis.CustomEvent,
            indexedDB: globalThis.indexedDB,
            fetch: globalThis.fetch,
        };
        Object.assign(globalThis, {
            window: windowInstance,
            sessionStorage: windowInstance.sessionStorage,
            CustomEvent: windowInstance.CustomEvent,
            indexedDB: createFakeIndexedDB(),
        });
        (/** @type {any} */ (windowInstance)).__ty_fetch_cache_db__ = null;
    });
    afterAll(async () => {
        await windowInstance.happyDOM.close();
        Object.assign(globalThis, previousGlobals);
    });
    test('isBrowser is true and isServer is false', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`__ty_test__.isBrowser = isBrowser; __ty_test__.isServer = isServer`);
        await factory();
        expect(r.isBrowser).toBe(true);
        expect(r.isServer).toBe(false);
    });
    test('onMount pushes callback to window.__ty_onMount_queue__', async () => {
        delete (/** @type {any} */ (windowInstance)).__ty_onMount_queue__;
        const r = testResults();
        r.called = false;
        const factory = await buildTestFactory(`onMount(() => { __ty_test__.called = true })`);
        await factory();
        const queue = /** @type {Array<() => void | Promise<void>>} */ ((/** @type {any} */ (windowInstance)).__ty_onMount_queue__);
        expect(Array.isArray(queue)).toBe(true);
        expect(queue.length).toBe(1);
        queue[0]();
        expect(r.called).toBe(true);
    });
    test('multiple onMount calls append to the queue in order', async () => {
        delete (/** @type {any} */ (windowInstance)).__ty_onMount_queue__;
        const r = testResults();
        r.order = [];
        const factory = await buildTestFactory(`
            onMount(() => { __ty_test__.order.push(1) })
            onMount(() => { __ty_test__.order.push(2) })
        `);
        await factory();
        const queue = /** @type {Array<() => void | Promise<void>>} */ ((/** @type {any} */ (windowInstance)).__ty_onMount_queue__);
        queue.forEach(/** @param {() => void | Promise<void>} fn */ (fn) => fn());
        expect(r.order).toEqual([1, 2]);
    });
    test('inject retrieves value from window.__ty_context__', async () => {
        const ctx = new Map([['apiBase', 'https://api.example.com']]);
        (/** @type {any} */ (windowInstance)).__ty_context__ = ctx;
        const r = testResults();
        const factory = await buildTestFactory(`__ty_test__.value = inject('apiBase')`);
        await factory();
        expect(r.value).toBe('https://api.example.com');
    });
    test('inject returns fallback for absent key', async () => {
        ;
        (/** @type {any} */ (windowInstance)).__ty_context__ = new Map();
        const r = testResults();
        const factory = await buildTestFactory(`__ty_test__.value = inject('missing', 'default')`);
        await factory();
        expect(r.value).toBe('default');
    });
    test('provide sets value in window.__ty_context__', async () => {
        const ctx = new Map();
        (/** @type {any} */ (windowInstance)).__ty_context__ = ctx;
        const factory = await buildTestFactory(`provide('svc', { url: '/api' })`);
        await factory();
        expect(ctx.get('svc')).toEqual({ url: '/api' });
    });
    test('public browser env values are exposed only through the explicit helper', async () => {
        (/** @type {any} */ (windowInstance)).__ty_public_env__ = {
            API_BASE_URL: 'https://api.example.com',
        };
        const r = testResults();
        const factory = await buildTestFactory(`
            const helpers = __ty_helpers__.createTacHelpers({})
            __ty_test__.value = helpers.env('API_BASE_URL', '/fallback')
            __ty_test__.missing = helpers.env('MISSING', '/fallback')
        `);
        await factory();
        expect(r.value).toBe('https://api.example.com');
        expect(r.missing).toBe('/fallback');
        delete (/** @type {any} */ (windowInstance)).__ty_public_env__;
    });
    test('persistent $ fields restore value from sessionStorage', async () => {
        windowInstance.sessionStorage.setItem('tac:__TY_MODULE_PATH__:fixture:$draft', JSON.stringify({ id: 7 }));
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $draft: null }
            const helpers = __ty_helpers__.createTacHelpers({ __ty_persist_id__: 'fixture' })
            helpers.bindPersistentFields(controller)
            __ty_test__.val = controller.$draft
        `);
        await factory();
        expect(r.val).toEqual({ id: 7 });
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$draft');
    });
    test('persistent $ fields keep their initial value when storage is empty', async () => {
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$draft');
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $draft: 'init' }
            const helpers = __ty_helpers__.createTacHelpers({ __ty_persist_id__: 'fixture' })
            helpers.bindPersistentFields(controller)
            __ty_test__.val = controller.$draft
        `);
        await factory();
        expect(r.val).toBe('init');
    });
    test('persistent $ field writes JSON to sessionStorage on assignment', async () => {
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$draft');
        const factory = await buildTestFactory(`
            const controller = { $draft: null }
            const helpers = __ty_helpers__.createTacHelpers({ __ty_persist_id__: 'fixture' })
            helpers.bindPersistentFields(controller)
            controller.$draft = { persisted: true }
        `);
        await factory();
        const stored = windowInstance.sessionStorage.getItem('tac:__TY_MODULE_PATH__:fixture:$draft');
        expect(JSON.parse(stored ?? 'null')).toEqual({ persisted: true });
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$draft');
    });
    test('rerender calls window.__ty_rerender', async () => {
        const r = testResults();
        r.called = false;
        (/** @type {any} */ (windowInstance)).__ty_rerender = () => { r.called = true; };
        const factory = await buildTestFactory(`rerender()`);
        await factory();
        expect(r.called).toBe(true);
        delete (/** @type {any} */ (windowInstance)).__ty_rerender;
    });
    test('rerender is safe when window.__ty_rerender is absent', async () => {
        delete (/** @type {any} */ (windowInstance)).__ty_rerender;
        const factory = await buildTestFactory(`rerender()`);
        expect(async () => await factory()).not.toThrow();
    });
    test('persistent $ fields are safe with malformed sessionStorage data', async () => {
        windowInstance.sessionStorage.setItem('tac:__TY_MODULE_PATH__:fixture:$draft', 'not-valid-json{{{');
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $draft: 'safe' }
            const helpers = __ty_helpers__.createTacHelpers({ __ty_persist_id__: 'fixture' })
            helpers.bindPersistentFields(controller)
            __ty_test__.val = controller.$draft
        `);
        await factory();
        expect(r.val).toBe('safe');
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$draft');
    });
    test('successful non-GET requests invalidate cached GET responses for the same URL', async () => {
        const r = testResults();
        (/** @type {any} */ (windowInstance)).__ty_fetch_cache_db__ = null;
        /** @type {string[]} */
        const seen = [];
        const fetchStub = /** @type {typeof fetch} */ (async (input, init) => {
            const request = new Request(input, init);
            const method = request.method.toUpperCase();
            seen.push(method);
            if (method === 'POST')
                return new Response('mutated', { status: 200 });
            if (seen.filter((entry) => entry === 'GET').length === 1)
                return new Response('cached-get', { status: 200 });
            return new Response('fresh-get', { status: 200 });
        });
        globalThis.fetch = fetchStub;
        (/** @type {any} */ (windowInstance)).fetch = fetchStub;
        const factory = await buildTestFactory(`
            async function run() {
                const helpers = __ty_helpers__.createTacHelpers({})
                const first = await helpers.fetch('https://example.test/items')
                const mutation = await helpers.fetch('https://example.test/items', { method: 'POST', body: 'name=widget' })
                const second = await helpers.fetch('https://example.test/items')
                __ty_test__.first = await first.text()
                __ty_test__.mutation = await mutation.text()
                __ty_test__.second = await second.text()
            }
            await run()
        `);
        await factory();
        expect(r.first).toBe('cached-get');
        expect(r.mutation).toBe('mutated');
        expect(r.second).toBe('fresh-get');
    });
});
// ── Prerender-environment (window = globalThis, no sessionStorage) ─────────────
describe('render-template prerender-environment (window=globalThis, __ty_prerender__=true)', () => {
    /** @type {Record<string, unknown>} */
    let previousGlobals;
    beforeAll(() => {
        previousGlobals = {
            window: globalThis.window,
            __ty_prerender__: globalThis.__ty_prerender__,
        };
        Object.assign(globalThis, {
            window: globalThis,
            __ty_prerender__: true,
        });
    });
    afterAll(() => {
        Object.assign(globalThis, previousGlobals);
        if (previousGlobals.__ty_prerender__ === undefined) {
            Reflect.deleteProperty(globalThis, '__ty_prerender__');
        }
    });
    test('isBrowser is false and isServer is true despite window being set', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`__ty_test__.isBrowser = isBrowser; __ty_test__.isServer = isServer`);
        await factory();
        expect(r.isBrowser).toBe(false);
        expect(r.isServer).toBe(true);
    });
    test('onMount does not push to any queue during prerender', async () => {
        const r = testResults();
        r.called = false;
        const factory = await buildTestFactory(`onMount(() => { __ty_test__.called = true })`);
        await factory();
        expect(r.called).toBe(false);
        expect((/** @type {any} */ (globalThis)).__ty_onMount_queue__).toBeUndefined();
    });
    test('persistent $ fields do not crash during prerender', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $draft: 99 }
            const helpers = __ty_helpers__.createTacHelpers({})
            helpers.bindPersistentFields(controller)
            controller.$draft = 100
            __ty_test__.val = controller.$draft
        `);
        await factory();
        expect(r.val).toBe(100);
    });
    test('inject returns fallback during prerender', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`__ty_test__.value = inject('k', 'prerender-fallback')`);
        await factory();
        expect(r.value).toBe('prerender-fallback');
    });
});
// ── Async event handler re-rendering ───────────────────────────────────────────
describe('ty_invokeEvent awaits async handlers', () => {
    test('state change after await inside handler is visible when render completes', async () => {
        const r = testResults();
        const script = `
            let step = 'start'
            async function next() {
                await new Promise(resolve => setTimeout(resolve, 50))
                step = 'done'
            }
        `;
        const inners = `
            await ty_invokeEvent('testhash', async ($event) => { const __event__ = $event; return next() })
            ;(globalThis).__ty_test__.step = step
        `;
        const factory = await buildTestFactory(script, inners);
        const render = await factory();
        // Trigger the event by passing the matching element ID
        await render('ty-testhash-0', null);
        expect(r.step).toBe('done');
    });
    test('sync handler still works with async callback and return', async () => {
        const r = testResults();
        const script = `
            let count = 0
        `;
        const inners = `
            await ty_invokeEvent('synchash', async ($event) => { const __event__ = $event; return count++ })
            ;(globalThis).__ty_test__.count = count
        `;
        const factory = await buildTestFactory(script, inners);
        const render = await factory();
        await render('ty-synchash-0', null);
        expect(r.count).toBe(1);
    });
    test('handler returning a promise chain is awaited', async () => {
        const r = testResults();
        const script = `
            let value = 'pending'
            function fetchData() {
                return new Promise(resolve => setTimeout(resolve, 30))
                    .then(() => { value = 'resolved' })
            }
        `;
        const inners = `
            await ty_invokeEvent('chainhash', async ($event) => { const __event__ = $event; return fetchData() })
            ;(globalThis).__ty_test__.value = value
        `;
        const factory = await buildTestFactory(script, inners);
        const render = await factory();
        await render('ty-chainhash-0', null);
        expect(r.value).toBe('resolved');
    });
    test('non-matching element ID does not execute handler', async () => {
        const r = testResults();
        r.called = false;
        const script = `
            async function handler() {
                ;(globalThis).__ty_test__.called = true
            }
        `;
        const inners = `
            await ty_invokeEvent('skiphash', async ($event) => { const __event__ = $event; return handler() })
        `;
        const factory = await buildTestFactory(script, inners);
        const render = await factory();
        // Pass a non-matching ID
        await render('ty-wrongid-0', null);
        expect(r.called).toBe(false);
    });
});
