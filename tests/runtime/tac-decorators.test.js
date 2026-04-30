// @ts-check
import { describe, expect, test } from 'bun:test';
import Tac from '../../src/runtime/tac.js';
import { inject, provide, env, onMount, emit, render } from '../../src/runtime/decorators.js';

/**
 * @typedef {import('../../src/runtime/tac.js').TacRuntimeBindings} TacRuntimeBindings
 */

/**
 * @returns {TacRuntimeBindings & {
 *   store: Map<string, unknown>,
 *   envs: Map<string, unknown>,
 *   mountFns: Array<() => void | Promise<void>>,
 *   emitted: Array<{ name: string, detail: unknown }>,
 *   rerenderCount: { value: number },
 * }}
 */
function createHelpers() {
    const store = new Map();
    const envs = new Map([['PORT', 4242]]);
    /** @type {Array<() => void | Promise<void>>} */
    const mountFns = [];
    /** @type {Array<{ name: string, detail: unknown }>} */
    const emitted = [];
    const rerenderCount = { value: 0 };
    return {
        store,
        envs,
        mountFns,
        emitted,
        rerenderCount,
        isBrowser: false,
        isServer: true,
        props: {},
        bindPersistentFields() { },
        env: (key, fallback) => /** @type {any} */ (envs.has(key) ? envs.get(key) : fallback),
        emit: (name, detail) => { emitted.push({ name, detail }); return true; },
        fetch: async () => new Response(''),
        inject: (key, fallback) => store.has(key) ? store.get(key) : fallback,
        onMount: (fn) => { mountFns.push(fn); },
        provide: (key, value) => { store.set(key, value); },
        rerender: () => { rerenderCount.value += 1; },
    };
}

describe('Tac decorators', () => {
    test('@inject reads from context with fallback', () => {
        class Fixture extends Tac {
            /** @type {string | undefined} */
            @inject('demo-release', 'Tac')
            release;
        }
        const helpers = createHelpers();
        helpers.store.set('demo-release', 'TACHYON 2.0.0');
        const ctrl = new Fixture({}, helpers);
        expect(ctrl.release).toBe('TACHYON 2.0.0');

        const fallback = new Fixture({}, createHelpers());
        expect(fallback.release).toBe('Tac');
    });

    test('@provide registers field value into context', () => {
        class Fixture extends Tac {
            @provide('demo-release')
            release = 'TACHYON 2.0.0';
        }
        const helpers = createHelpers();
        new Fixture({}, helpers);
        expect(helpers.store.get('demo-release')).toBe('TACHYON 2.0.0');
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

    test('@onMount registers method as mount handler', () => {
        let called = 0;
        class Fixture extends Tac {
            @onMount
            boot() { called += 1; }
        }
        const helpers = createHelpers();
        new Fixture({}, helpers);
        expect(helpers.mountFns).toHaveLength(1);
        helpers.mountFns[0]();
        expect(called).toBe(1);
    });

    test('@emit emits return value (sync)', () => {
        class Fixture extends Tac {
            @emit('saved')
            save() { return { id: 1 }; }
        }
        const helpers = createHelpers();
        const ctrl = new Fixture({}, helpers);
        const result = ctrl.save();
        expect(result).toEqual({ id: 1 });
        expect(helpers.emitted).toEqual([{ name: 'saved', detail: { id: 1 } }]);
    });

    test('@emit emits resolved value (async) and propagates rejection', async () => {
        class Fixture extends Tac {
            @emit('saved')
            async saveOk() { return { id: 2 }; }
            @emit('saved')
            async saveFail() { throw new Error('boom'); }
        }
        const helpers = createHelpers();
        const ctrl = new Fixture({}, helpers);
        await ctrl.saveOk();
        expect(helpers.emitted).toEqual([{ name: 'saved', detail: { id: 2 } }]);

        await expect(ctrl.saveFail()).rejects.toThrow('boom');
        expect(helpers.emitted).toHaveLength(1);
    });

    test('@render fires after a sync method returns', () => {
        class Fixture extends Tac {
            @render
            tick() { return 'ok'; }
        }
        const helpers = createHelpers();
        const ctrl = new Fixture({}, helpers);
        expect(ctrl.tick()).toBe('ok');
        expect(helpers.rerenderCount.value).toBe(1);
    });

    test('@render fires after a sync method throws and re-raises', () => {
        class Fixture extends Tac {
            @render
            boom() { throw new Error('nope'); }
        }
        const helpers = createHelpers();
        const ctrl = new Fixture({}, helpers);
        expect(() => ctrl.boom()).toThrow('nope');
        expect(helpers.rerenderCount.value).toBe(1);
    });

    test('@render fires after an async method resolves', async () => {
        class Fixture extends Tac {
            @render
            async work() { return 7; }
        }
        const helpers = createHelpers();
        const ctrl = new Fixture({}, helpers);
        await expect(ctrl.work()).resolves.toBe(7);
        expect(helpers.rerenderCount.value).toBe(1);
    });

    test('@render fires after an async method rejects and propagates', async () => {
        class Fixture extends Tac {
            @render
            async work() { throw new Error('async-boom'); }
        }
        const helpers = createHelpers();
        const ctrl = new Fixture({}, helpers);
        await expect(ctrl.work()).rejects.toThrow('async-boom');
        expect(helpers.rerenderCount.value).toBe(1);
    });
});
