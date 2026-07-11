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
        web: {
            fetch: async () => new Response(''),
            storage: {
                local: { get: (_key, fallback) => fallback, set: () => { }, remove: () => { } },
                session: { get: (_key, fallback) => fallback, set: () => { }, remove: () => { } },
            },
            navigator: { online: () => true, language: () => 'en', userAgent: () => '' },
            location: { href: () => '', origin: () => '' },
            clipboard: { readText: async () => '', writeText: async () => { } },
            sleep: async () => { },
        },
        deviceAvailable: () => false,
        device: async () => ({}),
        __native: {
            app: { available: () => false, info: async () => ({}) },
            clipboard: { available: () => false, readText: async () => '', writeText: async () => ({}) },
            browser: { available: () => false, open: async () => ({}) },
            filePicker: { available: () => false, openText: async () => ({}) },
            web: {
                localStorage: { getItem: (_key, fallback) => fallback, setItem: () => { }, removeItem: () => { } },
                sessionStorage: { getItem: (_key, fallback) => fallback, setItem: () => { }, removeItem: () => { } },
                navigator: { language: () => '', online: () => false, userAgent: () => '' },
                location: { href: () => '', origin: () => '' },
                fetch: async () => new Response(''),
            },
        },
        __nativeCall: async () => undefined,
        __nativeAvailable: () => false,
        __nativeCallback: (_operation, _payload, resolve) => resolve(undefined),
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
