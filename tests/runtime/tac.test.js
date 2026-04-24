// @ts-check
import { describe, expect, test } from 'bun:test';
import { Tac } from '../../src/server/route-handler.js';

/**
 * @typedef {import('../../src/runtime/tac.js').TacRuntimeBindings} TacRuntimeBindings
 */

/** @returns {TacRuntimeBindings & { session: Map<string, unknown> }} */
function createHelpers() {
    const store = new Map();
    const session = new Map();
    return {
        session,
        isBrowser: true,
        isServer: false,
        props: { label: 'fixture' },
        bindPersistentFields(controller) {
            for (const key of Object.keys(controller)) {
                if (!key.startsWith('$'))
                    continue;
                let current = session.has(key) ? session.get(key) : controller[key];
                Object.defineProperty(controller, key, {
                    configurable: true,
                    enumerable: true,
                    get() {
                        return current;
                    },
                    set(value) {
                        current = value;
                        session.set(key, value);
                    },
                });
                controller[key] = current;
            }
        },
        /** @template T @param {string} _key @param {T} [fallback] */
        env: (_key, fallback) => fallback,
        /** @param {string} name @param {unknown} detail */
        emit: (name, detail) => name === 'chosen' && detail === 7,
        fetch: async () => new Response(JSON.stringify({ ok: true }), { status: 200 }),
        /** @param {string} key @param {unknown} fallback */
        inject: (key, fallback) => store.has(key) ? store.get(key) : fallback,
        /** @param {() => void | Promise<void>} fn */
        onMount: (fn) => { void fn(); },
        /** @param {string} key @param {unknown} value */
        provide: (key, value) => { store.set(key, value); },
        rerender: () => { store.set('rerendered', true); },
    };
}

describe('Tac', () => {
    test('exposes bound Tac helpers through the base class and auto-binds $ fields', async () => {
        class Fixture extends Tac {
            $draft = 1
        }

        const helpers = createHelpers();
        const controller = new Fixture({ label: 'fixture' }, helpers);
        controller.__attachTacHelpers__(helpers);

        expect(controller.isBrowser).toBe(true);
        expect(controller.isServer).toBe(false);
        expect(controller.props).toEqual({ label: 'fixture' });
        controller.provide('apiBase', '/api');
        expect(controller.inject('apiBase', '')).toBe('/api');
        controller.$draft = 2;
        expect(helpers.session.get('$draft')).toBe(2);
        expect(controller.emit('chosen', 7)).toBe(true);
        controller.rerender();
        expect(controller.inject('rerendered', false)).toBe(true);
        const response = await controller.fetch('https://example.test');
        expect(response.status).toBe(200);
    });
});
