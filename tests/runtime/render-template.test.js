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
 * script slot. Results are returned via globalThis.__tc_test__ to bridge
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
const __tc_props__ = __tc_helpers__.decodeProps(props);
const __tc_scope__ = __tc_helpers__.createScope(null, __tc_props__);

with (__tc_scope__) {
    const fetch = __tc_helpers__.fetch;
    const isBrowser = __tc_helpers__.isBrowser;
    const isServer = __tc_helpers__.isServer;
    const onMount = __tc_helpers__.onMount;
    const publish = __tc_helpers__.publish;
    const rerender = __tc_helpers__.rerender;
    const subscribe = __tc_helpers__.subscribe;

    ${testScript}

    if (__tc_props__) {
        for (const __k__ of Object.keys(__tc_props__)) {
            if (/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(__k__) && !__k__.startsWith('__tc_')) {
                const __v__ = __tc_props__[__k__];
                try { eval(\`\${__k__} = __v__\`) } catch {}
            }
        }
    }

    const compRenders = new Map();

    return async function(elemId, event, compId) {
        const counters = { id: {}, ev: {}, bind: {} };
        const tc_componentRootId = compId
            ? (String(compId).startsWith('tc-') ? String(compId) : 'tc-' + compId + '-0')
            : null;

        __tc_helpers__.setRenderContext({ componentRootId: tc_componentRootId, elemId, event });

        const tc_generateId = (hash, source) => {
            const key = compId ? hash + '-' + compId : hash;
            const map = counters[source];

            if (key in map) return 'tc-' + key + '-' + map[key]++;

            map[key] = 1;
            return 'tc-' + key + '-0';
        };

        const tc_invokeEvent = async (hash, action) => {
            if (elemId === tc_generateId(hash, 'ev')) {
                if (typeof action === 'function') await action(event);
                else await eval(action);
            }
            return '';
        };

        const tc_assignValue = (hash, variable) => {
            if (elemId === tc_generateId(hash, 'bind') && event) {
                if (/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(variable)) {
                    const __val__ = event.value;
                    eval(\`\${variable} = __val__\`);
                }
            }
            return '';
        };

        const tc_escapeHtml = (value) => {
            if (value === null || value === undefined) return '';
            return String(value)
                .replaceAll('&', '&amp;')
                .replaceAll('<', '&lt;')
                .replaceAll('>', '&gt;')
                .replaceAll('"', '&quot;')
                .replaceAll("'", '&#39;');
        };

        const tc_escapeText = tc_escapeHtml;
        const tc_escapeAttr = tc_escapeHtml;

        let elements = '';
        let render;

        ${testInners ?? ''}

        return elements;
    };
}`;
    const modified = source
        .replace('// module_imports', '')
        .replace('"__TY_FACTORY_SOURCE__"', () => JSON.stringify(factorySource));
    const tmpPath = path.join(os.tmpdir(), `tachyon-tpl-${Bun.randomUUIDv7()}.js`);
    await Bun.write(tmpPath, modified);
    const { default: factory } = await import(tmpPath);
    return factory;
}

/** @returns {TestResults} */
function testResults() {
    /** @type {TestResults} */
    const results = {};
    /** @type {any} */ (globalThis).__tc_test__ = results;
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
        const factory = await buildTestFactory(`__tc_test__.isBrowser = isBrowser; __tc_test__.isServer = isServer`);
        await factory();
        expect(r.isBrowser).toBe(false);
        expect(r.isServer).toBe(true);
    });
    test('onMount is a no-op — callback is never registered or called', async () => {
        const r = testResults();
        r.called = false;
        const factory = await buildTestFactory(`onMount(() => { __tc_test__.called = true })`);
        await factory();
        expect(r.called).toBe(false);
    });
    test('subscribe returns fallback when window is absent', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`__tc_test__.value = subscribe('key', 'fallback')`);
        await factory();
        expect(r.value).toBe('fallback');
    });
    test('subscribe returns undefined fallback by default', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`__tc_test__.value = subscribe('key')`);
        await factory();
        expect(r.value).toBeUndefined();
    });
    test('env returns fallback when window is absent', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`
            const helpers = __tc_helpers__.createTacHelpers({})
            __tc_test__.value = helpers.env('API_BASE_URL', '/fallback')
        `);
        await factory();
        expect(r.value).toBe('/fallback');
    });
    test('persistent $ fields keep their default value without window', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $draft: 42 }
            const helpers = __tc_helpers__.createTacHelpers({})
            helpers.bindPersistentFields(controller)
            controller.$draft = 99
            __tc_test__.val = controller.$draft
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
            localStorage: globalThis.localStorage,
            CustomEvent: globalThis.CustomEvent,
            indexedDB: globalThis.indexedDB,
            fetch: globalThis.fetch,
            __tc_browser_cache__: /** @type {any} */ (globalThis).__tc_browser_cache__,
            __tc_fetch_installed__: /** @type {any} */ (globalThis).__tc_fetch_installed__,
            __tc_native_fetch__: /** @type {any} */ (globalThis).__tc_native_fetch__,
        };
        Object.assign(globalThis, {
            window: windowInstance,
            sessionStorage: windowInstance.sessionStorage,
            localStorage: windowInstance.localStorage,
            CustomEvent: windowInstance.CustomEvent,
            indexedDB: createFakeIndexedDB(),
        });
        (/** @type {any} */ (windowInstance)).__tc_fetch_cache_db__ = null;
    });
    afterAll(async () => {
        await windowInstance.happyDOM.close();
        Object.assign(globalThis, previousGlobals);
        for (const key of ['__tc_browser_cache__', '__tc_fetch_installed__', '__tc_native_fetch__']) {
            if (previousGlobals[key] === undefined)
                Reflect.deleteProperty(globalThis, key);
        }
    });
    test('isBrowser is true and isServer is false', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`__tc_test__.isBrowser = isBrowser; __tc_test__.isServer = isServer`);
        await factory();
        expect(r.isBrowser).toBe(true);
        expect(r.isServer).toBe(false);
    });
    test('onMount pushes callback to window.__tc_onMount_queue__', async () => {
        delete (/** @type {any} */ (windowInstance)).__tc_onMount_queue__;
        const r = testResults();
        r.called = false;
        const factory = await buildTestFactory(`onMount(() => { __tc_test__.called = true })`);
        await factory();
        const queue = /** @type {Array<() => void | Promise<void>>} */ ((/** @type {any} */ (windowInstance)).__tc_onMount_queue__);
        expect(Array.isArray(queue)).toBe(true);
        expect(queue.length).toBe(1);
        queue[0]();
        expect(r.called).toBe(true);
    });
    test('multiple onMount calls append to the queue in order', async () => {
        delete (/** @type {any} */ (windowInstance)).__tc_onMount_queue__;
        const r = testResults();
        r.order = [];
        const factory = await buildTestFactory(`
            onMount(() => { __tc_test__.order.push(1) })
            onMount(() => { __tc_test__.order.push(2) })
        `);
        await factory();
        const queue = /** @type {Array<() => void | Promise<void>>} */ ((/** @type {any} */ (windowInstance)).__tc_onMount_queue__);
        queue.forEach(/** @param {() => void | Promise<void>} fn */ (fn) => fn());
        expect(r.order).toEqual([1, 2]);
    });
    test('subscribe retrieves retained signal value', async () => {
        (/** @type {any} */ (windowInstance)).__tc_signals__ = {
            values: new Map([['apiBase', 'https://api.example.com']]),
            listeners: new Map(),
        };
        const r = testResults();
        const factory = await buildTestFactory(`__tc_test__.value = subscribe('apiBase')`);
        await factory();
        expect(r.value).toBe('https://api.example.com');
    });
    test('subscribe returns fallback for absent signal', async () => {
        (/** @type {any} */ (windowInstance)).__tc_signals__ = {
            values: new Map(),
            listeners: new Map(),
        };
        const r = testResults();
        const factory = await buildTestFactory(`__tc_test__.value = subscribe('missing', 'default')`);
        await factory();
        expect(r.value).toBe('default');
    });
    test('publish stores retained signal values', async () => {
        delete (/** @type {any} */ (windowInstance)).__tc_signals__;
        const factory = await buildTestFactory(`publish('svc', { url: '/api' }, { retain: true })`);
        await factory();
        expect((/** @type {any} */ (windowInstance)).__tc_signals__.values.get('svc')).toEqual({ url: '/api' });
    });
    test('publish notifies subscribers', async () => {
        delete (/** @type {any} */ (windowInstance)).__tc_signals__;
        const r = testResults();
        r.events = [];
        const factory = await buildTestFactory(`
            subscribe('saved', (value) => { __tc_test__.events.push(value) }, { immediate: false })
            publish('saved', { id: 1 })
        `);
        await factory();
        expect(r.events).toEqual([{ id: 1 }]);
    });
    test('published companion fields retain initial values and future assignments', async () => {
        delete (/** @type {any} */ (windowInstance)).__tc_signals__;
        const factory = await buildTestFactory(`
            const controller = {
                theme: 'light',
                __tc_signal_publish_fields__: [{ name: 'theme', field: 'theme', options: { retain: true } }]
            }
            const helpers = __tc_helpers__.createTacHelpers({})
            __tc_helpers__.bindCompanion(controller, {}, helpers)
            __tc_test__.initial = subscribe('theme')
            controller.theme = 'dark'
            __tc_test__.updated = subscribe('theme')
        `);
        const r = testResults();
        await factory();
        expect(r.initial).toBe('light');
        expect(r.updated).toBe('dark');
    });
    test('public browser env values are exposed only through the explicit helper', async () => {
        (/** @type {any} */ (windowInstance)).__tc_public_env__ = {
            API_BASE_URL: 'https://api.example.com',
        };
        const r = testResults();
        const factory = await buildTestFactory(`
            const helpers = __tc_helpers__.createTacHelpers({})
            __tc_test__.value = helpers.env('API_BASE_URL', '/fallback')
            __tc_test__.missing = helpers.env('MISSING', '/fallback')
        `);
        await factory();
        expect(r.value).toBe('https://api.example.com');
        expect(r.missing).toBe('/fallback');
        delete (/** @type {any} */ (windowInstance)).__tc_public_env__;
    });
    test('persistent $ fields restore value from sessionStorage', async () => {
        windowInstance.sessionStorage.setItem('tac:__TY_MODULE_PATH__:fixture:$draft', JSON.stringify({ id: 7 }));
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $draft: null }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture' })
            helpers.bindPersistentFields(controller)
            __tc_test__.val = controller.$draft
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
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture' })
            helpers.bindPersistentFields(controller)
            __tc_test__.val = controller.$draft
        `);
        await factory();
        expect(r.val).toBe('init');
    });
    test('persistent $ field writes JSON to sessionStorage on assignment', async () => {
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$draft');
        const factory = await buildTestFactory(`
            const controller = { $draft: null }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture' })
            helpers.bindPersistentFields(controller)
            controller.$draft = { persisted: true }
        `);
        await factory();
        const stored = windowInstance.sessionStorage.getItem('tac:__TY_MODULE_PATH__:fixture:$draft');
        expect(JSON.parse(stored ?? 'null')).toEqual({ persisted: true });
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$draft');
    });
    test('persistent $ fields use unprefixed props as defaults without overwriting stored values', async () => {
        windowInstance.sessionStorage.setItem('tac:__TY_MODULE_PATH__:fixture:$draft', JSON.stringify('stored'));
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $draft: 'init' }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture', draft: 'prop-default' })
            __tc_helpers__.bindCompanion(controller, helpers.props, helpers)
            __tc_test__.val = controller.$draft
        `);
        await factory();
        expect(r.val).toBe('stored');
        expect(JSON.parse(windowInstance.sessionStorage.getItem('tac:__TY_MODULE_PATH__:fixture:$draft') ?? 'null')).toBe('stored');
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$draft');
    });
    test('persistent $ fields fall back to unprefixed props when storage is empty', async () => {
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$draft');
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $draft: 'init' }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture', draft: 'prop-default' })
            __tc_helpers__.bindCompanion(controller, helpers.props, helpers)
            __tc_test__.val = controller.$draft
        `);
        await factory();
        expect(r.val).toBe('prop-default');
        expect(JSON.parse(windowInstance.sessionStorage.getItem('tac:__TY_MODULE_PATH__:fixture:$draft') ?? 'null')).toBe('prop-default');
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$draft');
    });
    test('companion public field assignments schedule one batched rerender', async () => {
        const r = testResults();
        r.calls = 0;
        (/** @type {any} */ (windowInstance)).__tc_rerender = () => { r.calls += 1; };
        const factory = await buildTestFactory(`
            const controller = { count: 0, label: 'idle' }
            const helpers = __tc_helpers__.createTacHelpers({})
            __tc_helpers__.bindCompanion(controller, helpers.props, helpers)
            controller.count = 1
            controller.label = 'ready'
            __tc_test__.count = controller.count
            __tc_test__.label = controller.label
        `);
        await factory();
        expect(r.count).toBe(1);
        expect(r.label).toBe('ready');
        expect(r.calls).toBe(1);
        delete (/** @type {any} */ (windowInstance)).__tc_rerender;
    });
    test('companion persistent field assignments write storage and schedule rerender', async () => {
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$draft');
        const r = testResults();
        r.calls = 0;
        (/** @type {any} */ (windowInstance)).__tc_rerender = () => { r.calls += 1; };
        const factory = await buildTestFactory(`
            const controller = { $draft: 'init' }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture' })
            __tc_helpers__.bindCompanion(controller, helpers.props, helpers)
            controller.$draft = 'saved'
            __tc_test__.value = controller.$draft
        `);
        await factory();
        expect(r.value).toBe('saved');
        expect(JSON.parse(windowInstance.sessionStorage.getItem('tac:__TY_MODULE_PATH__:fixture:$draft') ?? 'null')).toBe('saved');
        expect(r.calls).toBe(1);
        delete (/** @type {any} */ (windowInstance)).__tc_rerender;
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$draft');
    });
    test('companion field assignments during event render do not schedule an extra rerender', async () => {
        const r = testResults();
        r.calls = 0;
        (/** @type {any} */ (windowInstance)).__tc_rerender = () => { r.calls += 1; };
        const factory = await buildTestFactory(`
            const controller = { count: 0 }
            const helpers = __tc_helpers__.createTacHelpers({})
            __tc_helpers__.bindCompanion(controller, helpers.props, helpers)
            __tc_helpers__.setRenderContext({ elemId: 'tc-event-0' })
            controller.count = 1
            __tc_helpers__.setRenderContext({ elemId: null })
        `);
        await factory();
        await Promise.resolve();
        expect(r.calls).toBe(0);
        delete (/** @type {any} */ (windowInstance)).__tc_rerender;
    });
    test('rerender calls window.__tc_rerender', async () => {
        const r = testResults();
        r.called = false;
        (/** @type {any} */ (windowInstance)).__tc_rerender = () => { r.called = true; };
        const factory = await buildTestFactory(`rerender()`);
        await factory();
        expect(r.called).toBe(true);
        delete (/** @type {any} */ (windowInstance)).__tc_rerender;
    });
    test('rerender is safe when window.__tc_rerender is absent', async () => {
        delete (/** @type {any} */ (windowInstance)).__tc_rerender;
        const factory = await buildTestFactory(`rerender()`);
        expect(async () => await factory()).not.toThrow();
    });
    test('persistent $ fields are safe with malformed sessionStorage data', async () => {
        windowInstance.sessionStorage.setItem('tac:__TY_MODULE_PATH__:fixture:$draft', 'not-valid-json{{{');
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $draft: 'safe' }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture' })
            helpers.bindPersistentFields(controller)
            __tc_test__.val = controller.$draft
        `);
        await factory();
        expect(r.val).toBe('safe');
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$draft');
    });
    test('persistent $$ fields restore value from localStorage', async () => {
        windowInstance.localStorage.setItem('tac:__TY_MODULE_PATH__:fixture:$$theme', JSON.stringify('dark'));
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $$theme: 'light' }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture' })
            helpers.bindPersistentFields(controller)
            __tc_test__.val = controller.$$theme
        `);
        await factory();
        expect(r.val).toBe('dark');
        windowInstance.localStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$$theme');
    });
    test('persistent $$ fields keep their initial value when localStorage is empty', async () => {
        windowInstance.localStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$$theme');
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $$theme: 'system' }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture' })
            helpers.bindPersistentFields(controller)
            __tc_test__.val = controller.$$theme
        `);
        await factory();
        expect(r.val).toBe('system');
    });
    test('persistent $$ field writes JSON to localStorage on assignment', async () => {
        windowInstance.localStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$$theme');
        const factory = await buildTestFactory(`
            const controller = { $$theme: 'light' }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture' })
            helpers.bindPersistentFields(controller)
            controller.$$theme = 'oled'
        `);
        await factory();
        const stored = windowInstance.localStorage.getItem('tac:__TY_MODULE_PATH__:fixture:$$theme');
        expect(JSON.parse(stored ?? 'null')).toBe('oled');
        windowInstance.localStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$$theme');
    });
    test('persistent $$ fields use unprefixed props as defaults without overwriting stored values', async () => {
        windowInstance.localStorage.setItem('tac:__TY_MODULE_PATH__:fixture:$$theme', JSON.stringify('stored'));
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $$theme: 'light' }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture', theme: 'prop-theme' })
            __tc_helpers__.bindCompanion(controller, helpers.props, helpers)
            __tc_test__.val = controller.$$theme
        `);
        await factory();
        expect(r.val).toBe('stored');
        expect(JSON.parse(windowInstance.localStorage.getItem('tac:__TY_MODULE_PATH__:fixture:$$theme') ?? 'null')).toBe('stored');
        windowInstance.localStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$$theme');
    });
    test('persistent $$ fields fall back to unprefixed props when localStorage is empty', async () => {
        windowInstance.localStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$$theme');
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $$theme: 'light' }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture', theme: 'prop-theme' })
            __tc_helpers__.bindCompanion(controller, helpers.props, helpers)
            __tc_test__.val = controller.$$theme
        `);
        await factory();
        expect(r.val).toBe('prop-theme');
        expect(JSON.parse(windowInstance.localStorage.getItem('tac:__TY_MODULE_PATH__:fixture:$$theme') ?? 'null')).toBe('prop-theme');
        windowInstance.localStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$$theme');
    });
    test('persistent $$ field assignments write localStorage and schedule rerender', async () => {
        windowInstance.localStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$$theme');
        const r = testResults();
        r.calls = 0;
        (/** @type {any} */ (windowInstance)).__tc_rerender = () => { r.calls += 1; };
        const factory = await buildTestFactory(`
            const controller = { $$theme: 'light' }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture' })
            __tc_helpers__.bindCompanion(controller, helpers.props, helpers)
            controller.$$theme = 'dark'
            __tc_test__.value = controller.$$theme
        `);
        await factory();
        expect(r.value).toBe('dark');
        expect(JSON.parse(windowInstance.localStorage.getItem('tac:__TY_MODULE_PATH__:fixture:$$theme') ?? 'null')).toBe('dark');
        expect(r.calls).toBe(1);
        delete (/** @type {any} */ (windowInstance)).__tc_rerender;
        windowInstance.localStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$$theme');
    });
    test('persistent $$ fields are safe with malformed localStorage data', async () => {
        windowInstance.localStorage.setItem('tac:__TY_MODULE_PATH__:fixture:$$theme', 'not-valid-json{{{');
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $$theme: 'safe' }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture' })
            helpers.bindPersistentFields(controller)
            __tc_test__.val = controller.$$theme
        `);
        await factory();
        expect(r.val).toBe('safe');
        windowInstance.localStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$$theme');
    });
    test('$ and $$ fields coexist without interfering', async () => {
        windowInstance.sessionStorage.setItem('tac:__TY_MODULE_PATH__:fixture:$clicks', JSON.stringify(5));
        windowInstance.localStorage.setItem('tac:__TY_MODULE_PATH__:fixture:$$theme', JSON.stringify('dark'));
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $clicks: 0, $$theme: 'light' }
            const helpers = __tc_helpers__.createTacHelpers({ __tc_persist_id__: 'fixture' })
            helpers.bindPersistentFields(controller)
            controller.$clicks += 1
            controller.$$theme = 'oled'
            __tc_test__.clicks = controller.$clicks
            __tc_test__.theme = controller.$$theme
        `);
        await factory();
        expect(r.clicks).toBe(6);
        expect(r.theme).toBe('oled');
        expect(JSON.parse(windowInstance.sessionStorage.getItem('tac:__TY_MODULE_PATH__:fixture:$clicks') ?? 'null')).toBe(6);
        expect(JSON.parse(windowInstance.localStorage.getItem('tac:__TY_MODULE_PATH__:fixture:$$theme') ?? 'null')).toBe('oled');
        windowInstance.sessionStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$clicks');
        windowInstance.localStorage.removeItem('tac:__TY_MODULE_PATH__:fixture:$$theme');
    });
    test('successful non-GET requests invalidate cached GET responses for the same URL', async () => {
        const r = testResults();
        (/** @type {any} */ (windowInstance)).__tc_fetch_cache_db__ = null;
        const previousNativeFetch = (/** @type {any} */ (globalThis)).__tc_native_fetch__;
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
        (/** @type {any} */ (globalThis)).__tc_native_fetch__ = fetchStub;
        try {
            const factory = await buildTestFactory(`
                async function run() {
                    const helpers = __tc_helpers__.createTacHelpers({})
                    const first = await helpers.fetch('https://example.test/items')
                    const mutation = await helpers.fetch('https://example.test/items', { method: 'POST', body: 'name=widget' })
                    const second = await helpers.fetch('https://example.test/items')
                    __tc_test__.first = await first.text()
                    __tc_test__.mutation = await mutation.text()
                    __tc_test__.second = await second.text()
                }
                await run()
            `);
            await factory();
        } finally {
            if (previousNativeFetch === undefined) {
                Reflect.deleteProperty(globalThis, '__tc_native_fetch__');
            } else {
                (/** @type {any} */ (globalThis)).__tc_native_fetch__ = previousNativeFetch;
            }
        }
        expect(r.first).toBe('cached-get');
        expect(r.mutation).toBe('mutated');
        expect(r.second).toBe('fresh-get');
    });
});
// ── Prerender-environment (window = globalThis, no sessionStorage) ─────────────
describe('render-template prerender-environment (window=globalThis, __tc_prerender__=true)', () => {
    /** @type {Record<string, unknown>} */
    let previousGlobals;
    beforeAll(() => {
        previousGlobals = {
            window: globalThis.window,
            __tc_prerender__: /** @type {any} */ (globalThis).__tc_prerender__,
        };
        Object.assign(globalThis, {
            window: globalThis,
            __tc_prerender__: true,
        });
    });
    afterAll(() => {
        Object.assign(globalThis, previousGlobals);
        if (previousGlobals.__tc_prerender__ === undefined) {
            Reflect.deleteProperty(globalThis, '__tc_prerender__');
        }
    });
    test('isBrowser is false and isServer is true despite window being set', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`__tc_test__.isBrowser = isBrowser; __tc_test__.isServer = isServer`);
        await factory();
        expect(r.isBrowser).toBe(false);
        expect(r.isServer).toBe(true);
    });
    test('onMount does not push to any queue during prerender', async () => {
        const r = testResults();
        r.called = false;
        const factory = await buildTestFactory(`onMount(() => { __tc_test__.called = true })`);
        await factory();
        expect(r.called).toBe(false);
        expect((/** @type {any} */ (globalThis)).__tc_onMount_queue__).toBeUndefined();
    });
    test('persistent $ fields do not crash during prerender', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $draft: 99 }
            const helpers = __tc_helpers__.createTacHelpers({})
            helpers.bindPersistentFields(controller)
            controller.$draft = 100
            __tc_test__.val = controller.$draft
        `);
        await factory();
        expect(r.val).toBe(100);
    });
    test('persistent $$ fields do not crash during prerender', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`
            const controller = { $$theme: 'dark' }
            const helpers = __tc_helpers__.createTacHelpers({})
            helpers.bindPersistentFields(controller)
            controller.$$theme = 'light'
            __tc_test__.val = controller.$$theme
        `);
        await factory();
        expect(r.val).toBe('light');
    });
    test('subscribe returns fallback during prerender', async () => {
        const r = testResults();
        const factory = await buildTestFactory(`__tc_test__.value = subscribe('k', 'prerender-fallback')`);
        await factory();
        expect(r.value).toBe('prerender-fallback');
    });
});
// ── Async event handler re-rendering ───────────────────────────────────────────
describe('tc_invokeEvent awaits async handlers', () => {
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
            await tc_invokeEvent('testhash', async ($event) => { const __event__ = $event; return next() })
            ;(globalThis).__tc_test__.step = step
        `;
        const factory = await buildTestFactory(script, inners);
        const render = await factory();
        // Trigger the event by passing the matching element ID
        await render('tc-testhash-0', null);
        expect(r.step).toBe('done');
    });
    test('sync handler still works with async callback and return', async () => {
        const r = testResults();
        const script = `
            let count = 0
        `;
        const inners = `
            await tc_invokeEvent('synchash', async ($event) => { const __event__ = $event; return count++ })
            ;(globalThis).__tc_test__.count = count
        `;
        const factory = await buildTestFactory(script, inners);
        const render = await factory();
        await render('tc-synchash-0', null);
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
            await tc_invokeEvent('chainhash', async ($event) => { const __event__ = $event; return fetchData() })
            ;(globalThis).__tc_test__.value = value
        `;
        const factory = await buildTestFactory(script, inners);
        const render = await factory();
        await render('tc-chainhash-0', null);
        expect(r.value).toBe('resolved');
    });
    test('non-matching element ID does not execute handler', async () => {
        const r = testResults();
        r.called = false;
        const script = `
            async function handler() {
                ;(globalThis).__tc_test__.called = true
            }
        `;
        const inners = `
            await tc_invokeEvent('skiphash', async ($event) => { const __event__ = $event; return handler() })
        `;
        const factory = await buildTestFactory(script, inners);
        const render = await factory();
        // Pass a non-matching ID
        await render('tc-wrongid-0', null);
        expect(r.called).toBe(false);
    });
});

describe('delegateEvents helper registers the compile-time event set', () => {
    test('routes the event set to the Tac runtime once at factory setup', async () => {
        /** @type {string[]} */
        const captured = [];
        const previousTac = /** @type {any} */ (globalThis).Tac;
        /** @type {any} */ (globalThis).Tac = { delegateEvents: (/** @type {string[]} */ names) => captured.push(...names) };
        try {
            // Mirrors the call the compiler injects into a module's factory setup.
            const factory = await buildTestFactory(`__tc_helpers__.delegateEvents(['save', 'update:selected']);`);
            await factory();
            expect(captured).toEqual(['save', 'update:selected']);
        }
        finally {
            /** @type {any} */ (globalThis).Tac = previousTac;
        }
    });

    test('is a no-op when no Tac runtime is present (server prerender)', async () => {
        const previousTac = /** @type {any} */ (globalThis).Tac;
        /** @type {any} */ (globalThis).Tac = undefined;
        try {
            const factory = await buildTestFactory(`__tc_helpers__.delegateEvents(['save']);`);
            await expect(factory()).resolves.toBeDefined(); // does not throw
        }
        finally {
            /** @type {any} */ (globalThis).Tac = previousTac;
        }
    });
});

describe('registerComponentRender helper routes to the Tac runtime', () => {
    test('forwards (hostId, render, compId) to the runtime registry', async () => {
        /** @type {any[]} */
        const calls = [];
        const previousTac = /** @type {any} */ (globalThis).Tac;
        /** @type {any} */ (globalThis).Tac = {
            registerComponentRender: (/** @type {any} */ h, /** @type {any} */ r, /** @type {any} */ c) => calls.push([h, typeof r, c]),
        };
        try {
            const factory = await buildTestFactory(`__tc_helpers__.registerComponentRender('tc-abc-0', async () => '', 'abc');`);
            await factory();
            expect(calls).toEqual([['tc-abc-0', 'function', 'abc']]);
        }
        finally {
            /** @type {any} */ (globalThis).Tac = previousTac;
        }
    });

    test('is a no-op during server prerender (no Tac runtime)', async () => {
        const previousTac = /** @type {any} */ (globalThis).Tac;
        /** @type {any} */ (globalThis).Tac = undefined;
        try {
            const factory = await buildTestFactory(`__tc_helpers__.registerComponentRender('tc-abc-0', async () => '', 'abc');`);
            await expect(factory()).resolves.toBeDefined();
        }
        finally {
            /** @type {any} */ (globalThis).Tac = previousTac;
        }
    });
});
