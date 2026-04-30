// @ts-check
//
// Compiler-injected `fylo` global for Tac companion scripts.
//
// imports.js sets `window.fylo` to a property-access Proxy at boot, but
// companion scripts compile to ES modules where bare identifiers don't auto-
// resolve to `window.<name>`. This module exports a `fylo` symbol the
// compiler prepends as `import { fylo } from '../runtime/fylo-global.js'` when
// it detects a bare reference (the same pattern as decorators / Tac).
//
// We delegate to `window.fylo` at access time, not at module load — so the
// timing of imports.js bootstrap relative to a lazy-loaded component doesn't
// matter. If imports.js never ran (or `/_fylo` isn't mounted), every method
// returns a graceful "not enabled" error.

const noopCollection = {
    /** @returns {Promise<{ error: string }>} */
    async find() { return { error: 'Fylo browser not enabled' }; },
    /** @returns {Promise<{ error: string }>} */
    async list() { return { error: 'Fylo browser not enabled' }; },
    /** @returns {Promise<{ error: string }>} */
    async get() { return { error: 'Fylo browser not enabled' }; },
    /** @returns {Promise<{ error: string }>} */
    async events() { return { error: 'Fylo browser not enabled' }; },
    /** @returns {Promise<{ error: string }>} */
    async patch() { return { error: 'Fylo browser not enabled' }; },
    /** @returns {Promise<{ error: string }>} */
    async del() { return { error: 'Fylo browser not enabled' }; },
    /** @returns {Promise<{ error: string }>} */
    async rebuild() { return { error: 'Fylo browser not enabled' }; },
};

const noopBase = {
    enabled: false,
    /** @type {string | undefined} */
    root: undefined,
    /** @returns {Promise<{ error: string }>} */
    async sql() { return { error: 'Fylo browser not enabled' }; },
    /** @returns {Promise<{ root: string, collections: unknown[] }>} */
    async collections() { return { root: '', collections: [] }; },
    /** @returns {Promise<null>} */
    async meta() { return null; },
};

const noopProxy = new Proxy(noopBase, {
    get(target, prop) {
        if (typeof prop === 'string' && !(prop in target)) return noopCollection;
        return Reflect.get(target, prop);
    },
});

/**
 * Lazy delegate — resolves to `window.fylo` if present, else the noop Proxy.
 * Property access is per-call so a late-mounting browser global is picked up.
 */
export const fylo = /** @type {any} */ (new Proxy(noopProxy, {
    get(_target, prop) {
        const live = (typeof window !== 'undefined' && /** @type {any} */ (window).fylo) || noopProxy;
        return Reflect.get(live, prop);
    },
    has(_target, prop) {
        const live = (typeof window !== 'undefined' && /** @type {any} */ (window).fylo) || noopProxy;
        return Reflect.has(live, prop);
    },
}));
