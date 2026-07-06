// @ts-check
import { describe, expect, test } from 'bun:test';
import Tac from '../../src/runtime/tac.js';
import { subscribe, publish, env, onMount } from '../../src/runtime/decorators.js';

/**
 * @typedef {import('../../src/runtime/tac.js').TacRuntimeBindings} TacRuntimeBindings
 */

/**
 * @returns {TacRuntimeBindings & {
 *   values: Map<string, unknown>,
 *   listeners: Map<string, Set<(value: unknown) => void | Promise<void>>>,
 *   envs: Map<string, unknown>,
 *   mountFns: Array<() => void | Promise<void>>,
 * }}
 */
function createHelpers() {
    const values = new Map();
    /** @type {Map<string, Set<(value: unknown) => void | Promise<void>>>} */
    const listeners = new Map();
    const envs = new Map([['PORT', 4242]]);
    /** @type {Array<() => void | Promise<void>>} */
    const mountFns = [];
    return {
        values,
        listeners,
        envs,
        mountFns,
        isBrowser: false,
        isServer: true,
        props: {},
        bindPersistentFields() { },
        env: (key, fallback) => /** @type {any} */ (envs.has(key) ? envs.get(key) : fallback),
        fetch: async () => new Response(''),
        onMount: (fn) => { mountFns.push(fn); },
        publish: (name, value, options = {}) => {
            if (options.retain) values.set(name, value);
            for (const listener of listeners.get(name) ?? []) void listener(value);
            return (listeners.get(name)?.size ?? 0) > 0;
        },
        rerender: () => { },
        subscribe: (name, callbackOrFallback, options = {}) => {
            if (typeof callbackOrFallback !== 'function') {
                return values.has(name) ? values.get(name) : callbackOrFallback;
            }
            let signalListeners = listeners.get(name);
            if (!signalListeners) {
                signalListeners = new Set();
                listeners.set(name, signalListeners);
            }
            signalListeners.add(callbackOrFallback);
            if (options.immediate !== false && values.has(name)) void callbackOrFallback(values.get(name));
            return () => signalListeners?.delete(callbackOrFallback);
        },
    };
}

describe('Tac decorators', () => {
    test('@subscribe field reads retained signal with fallback', async () => {
        class Fixture extends Tac {
            /** @type {string | undefined} */
            @subscribe('demo-release', 'Tac')
            release;
        }
        const helpers = createHelpers();
        helpers.values.set('demo-release', 'TACHYON 2.0.0');
        const ctrl = new Fixture({}, helpers);
        expect(ctrl.release).toBe('TACHYON 2.0.0');
        await Promise.resolve();
        helpers.publish('demo-release', 'TACHYON 2.1.0', { retain: true });
        expect(ctrl.release).toBe('TACHYON 2.1.0');

        const fallback = new Fixture({}, createHelpers());
        expect(fallback.release).toBe('Tac');
    });

    test('@subscribe field uses the field name as the default signal name', async () => {
        class Fixture extends Tac {
            /** @type {string | undefined} */
            @subscribe
            release;
        }
        const helpers = createHelpers();
        helpers.values.set('release', 'TACHYON 2.0.0');
        const ctrl = new Fixture({}, helpers);
        expect(ctrl.release).toBe('TACHYON 2.0.0');
        await Promise.resolve();
        helpers.publish('release', 'TACHYON 2.1.0', { retain: true });
        expect(ctrl.release).toBe('TACHYON 2.1.0');
    });

    test('@publish field registers retained signal value', () => {
        class Fixture extends Tac {
            @publish('demo-release')
            release = 'TACHYON 2.0.0';
        }
        const helpers = createHelpers();
        const controller = new Fixture({}, helpers);
        const fields = /** @type {Array<{ name: string, field: string, options: { retain: true } }>} */ (controller.__tc_signal_publish_fields__);
        expect(fields).toEqual([{ name: 'demo-release', field: 'release', options: { retain: true } }]);
    });

    test('@publish field uses the field name as the default signal name', () => {
        class Fixture extends Tac {
            @publish
            release = 'TACHYON 2.0.0';
        }
        const helpers = createHelpers();
        const controller = new Fixture({}, helpers);
        const fields = /** @type {Array<{ name: string, field: string, options: { retain: true } }>} */ (controller.__tc_signal_publish_fields__);
        expect(fields).toEqual([{ name: 'release', field: 'release', options: { retain: true } }]);
    });

    test('@env reads env helper with fallback', () => {
        class Fixture extends Tac {
            /** @type {number | undefined} */
            @env('PORT', 3000)
            port;
            /** @type {string | undefined} */
            @env('MISSING', 'default')
            missing;
        }
        const ctrl = new Fixture({}, createHelpers());
        expect(ctrl.port).toBe(4242);
        expect(ctrl.missing).toBe('default');
    });

    test('@onMount registers method as mount handler', async () => {
        let called = 0;
        class Fixture extends Tac {
            @onMount
            boot() { called += 1; }
        }
        const helpers = createHelpers();
        new Fixture({}, helpers);
        await Promise.resolve();
        expect(helpers.mountFns).toHaveLength(1);
        helpers.mountFns[0]();
        expect(called).toBe(1);
    });

    test('@onMount registers after renderer helper binding replaces constructor defaults', async () => {
        let called = 0;
        class Fixture extends Tac {
            constructor(props = {}) {
                super(props);
            }

            @onMount
            boot() { called += 1; }
        }
        const helpers = createHelpers();
        const controller = new Fixture();
        controller.tac = helpers;
        await Promise.resolve();
        expect(helpers.mountFns).toHaveLength(1);
        helpers.mountFns[0]();
        expect(called).toBe(1);
    });

    test('@publish method publishes return value (sync)', () => {
        class Fixture extends Tac {
            @publish('saved')
            save() { return { id: 1 }; }
        }
        const helpers = createHelpers();
        /** @type {unknown[]} */
        const received = [];
        helpers.subscribe('saved', (value) => { received.push(value); }, { immediate: false });
        const ctrl = new Fixture({}, helpers);
        const result = ctrl.save();
        expect(result).toEqual({ id: 1 });
        expect(received).toEqual([{ id: 1 }]);
    });

    test('@publish method uses the method name as the default signal name', () => {
        class Fixture extends Tac {
            @publish
            saved() { return { id: 5 }; }
        }
        const helpers = createHelpers();
        /** @type {unknown[]} */
        const received = [];
        helpers.subscribe('saved', (value) => { received.push(value); }, { immediate: false });
        const ctrl = new Fixture({}, helpers);
        const result = ctrl.saved();
        expect(result).toEqual({ id: 5 });
        expect(received).toEqual([{ id: 5 }]);
    });

    test('@publish method publishes resolved value (async) and propagates rejection', async () => {
        class Fixture extends Tac {
            @publish('saved')
            async saveOk() { return { id: 2 }; }
            @publish('saved')
            async saveFail() { throw new Error('boom'); }
        }
        const helpers = createHelpers();
        /** @type {unknown[]} */
        const received = [];
        helpers.subscribe('saved', (value) => { received.push(value); }, { immediate: false });
        const ctrl = new Fixture({}, helpers);
        await ctrl.saveOk();
        expect(received).toEqual([{ id: 2 }]);

        await expect(ctrl.saveFail()).rejects.toThrow('boom');
        expect(received).toHaveLength(1);
    });

    test('@subscribe method receives published values', async () => {
        /** @type {unknown[]} */
        const received = [];
        class Fixture extends Tac {
            @subscribe('saved')
            onSaved(value) { received.push(value); }
        }
        const helpers = createHelpers();
        new Fixture({}, helpers);
        await Promise.resolve();
        helpers.publish('saved', { id: 3 });
        expect(received).toEqual([{ id: 3 }]);
    });

    test('@subscribe method uses the method name as the default signal name', async () => {
        /** @type {unknown[]} */
        const received = [];
        class Fixture extends Tac {
            @subscribe
            saved(value) { received.push(value); }
        }
        const helpers = createHelpers();
        new Fixture({}, helpers);
        await Promise.resolve();
        helpers.publish('saved', { id: 6 });
        expect(received).toEqual([{ id: 6 }]);
    });

    test('@subscribe method can also run once on mount', async () => {
        /** @type {unknown[]} */
        const received = [];
        class Fixture extends Tac {
            @subscribe('saved', { onMount: true })
            onSaved(value) { received.push(value ?? 'mounted'); }
        }
        const helpers = createHelpers();
        new Fixture({}, helpers);
        await Promise.resolve();
        expect(helpers.mountFns).toHaveLength(1);
        expect(received).toEqual([]);

        helpers.mountFns[0]();
        expect(received).toEqual(['mounted']);

        helpers.publish('saved', { id: 4 });
        expect(received).toEqual(['mounted', { id: 4 }]);
    });

    test('@subscribe method with default signal name can run once on mount', async () => {
        /** @type {unknown[]} */
        const received = [];
        class Fixture extends Tac {
            @subscribe({ onMount: true })
            saved(value) { received.push(value ?? 'mounted'); }
        }
        const helpers = createHelpers();
        new Fixture({}, helpers);
        await Promise.resolve();
        expect(helpers.mountFns).toHaveLength(1);
        helpers.mountFns[0]();
        helpers.publish('saved', { id: 7 });
        expect(received).toEqual(['mounted', { id: 7 }]);
    });
});
