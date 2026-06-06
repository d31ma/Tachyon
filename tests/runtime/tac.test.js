// @ts-check
import { describe, expect, test } from 'bun:test';
import { Tac } from '../../src/server/http/route-handler.js';

/**
 * @typedef {import('../../src/runtime/tac.js').TacRuntimeBindings} TacRuntimeBindings
 */

/** @returns {TacRuntimeBindings} */
function createHelpers() {
    return {
        isBrowser: true,
        isServer: false,
        props: {},
        bindPersistentFields: () => { },
        env: (_key, fallback) => fallback,
        fetch: async () => new Response(''),
        onMount: () => { },
        publish: () => true,
        rerender: () => { },
        subscribe: (_name, callbackOrFallback) => typeof callbackOrFallback === 'function' ? () => { } : callbackOrFallback,
    };
}

describe('Tac', () => {
    test('constructor assigns props and tac onto the instance', () => {
        const helpers = createHelpers();
        const props = { label: 'fixture' };
        const instance = new Tac(props, helpers);
        expect(instance.props).toBe(props);
        expect(instance.tac).toBe(helpers);
    });

    test('constructor falls back to noop helpers when tac is omitted', () => {
        const instance = new Tac({});
        expect(instance.tac.isBrowser).toBe(false);
        expect(instance.tac.isServer).toBe(true);
    });

    test('constructor defaults props to an empty object', () => {
        const instance = new Tac();
        expect(instance.props).toEqual({});
    });

    test('subclass field initializers can read this.tac (decorators rely on this)', () => {
        class Fixture extends Tac {
            /** @type {string} */
            captured = this.tac.isBrowser ? 'browser' : 'server';
        }
        const instance = new Fixture({}, createHelpers());
        expect(instance.captured).toBe('browser');
    });
});
