// @ts-check
//
// Framework-level Fylo browser client for Tac companion scripts and templates.
//
// Provides `window.fylo` as a self-bootstrapping Proxy that lazily discovers the
// Fylo browser API path from `<meta name="fylo-browser-path">` (injected into
// every Tachyon shell by Compiler.renderShellHTML / Yon.renderShellHTML).
//
// Companion scripts compile to ES modules where bare identifiers don't auto-
// resolve to `window.<name>`. The compiler prepends
// `import { fylo } from '../runtime/fylo-global.js'` when it detects a bare
// `fylo` reference — same pattern as decorators / Tac.
//
// If the Fylo browser route isn't mounted (no `/_fylo` endpoint), every method
// returns a graceful "not enabled" error.

/** @type {string | null} */
let __fyloAuthHeader = null;

/**
 * Encodes Basic auth credentials as UTF-8 before Base64 conversion.
 * @param {string} user
 * @param {string} pass
 * @returns {string}
 */
function __fyloBasicAuth(user, pass) {
    const bytes = new TextEncoder().encode(`${user}:${pass}`);
    let binary = '';
    for (const byte of bytes) binary += String.fromCharCode(byte);
    return `Basic ${btoa(binary)}`;
}

/**
 * Resolves the Fylo browser API base path. Reads from the shell-injected
 * `<meta name="fylo-browser-path">` tag, falling back to `/_fylo`.
 * @returns {string}
 */
function resolveBrowserPath() {
    if (typeof document === 'undefined') return '/_fylo';
    try {
        const meta = /** @type {HTMLMetaElement | null} */ (
            document.querySelector('meta[name="fylo-browser-path"]')
        );
        return meta?.content || '/_fylo';
    } catch {
        return '/_fylo';
    }
}

/**
 * Internal fetch wrapper that injects the Authorization header when
 * credentials have been set via `fylo.setCredentials()`.
 * @param {string} url
 * @param {RequestInit} [init]
 * @returns {Promise<Response>}
 */
async function __fyloFetch(url, init = {}) {
    const headers = new Headers(init.headers || {});
    if (__fyloAuthHeader) headers.set('Authorization', __fyloAuthHeader);
    return fetch(url, { ...init, headers });
}

/**
 * @param {string} basePath
 * @param {string} apiPath
 * @param {Record<string, unknown>} body
 * @returns {Promise<any>}
 */
async function __fyloPostJson(basePath, apiPath, body) {
    const r = await __fyloFetch(`${basePath}${apiPath}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
    });
    return r.json();
}

/**
 * Creates a per-collection proxy exposing find, list, get, events, patch,
 * del, and rebuild methods.
 * @param {string} basePath
 * @param {string} collection
 */
function __fyloCollection(basePath, collection) {
    return {
        /** @param {Record<string, unknown>} [query] */
        async find(query = {}) {
            return __fyloPostJson(basePath, '/api/query', { kind: 'find', collection, query });
        },
        /** @param {number} [limit] */
        async list(limit = 25) {
            const r = await __fyloFetch(`${basePath}/api/docs?collection=${encodeURIComponent(collection)}&limit=${limit}`);
            return r.json();
        },
        /** @param {string} id */
        async get(id) {
            const r = await __fyloFetch(`${basePath}/api/doc?collection=${encodeURIComponent(collection)}&id=${encodeURIComponent(id)}`);
            return r.json();
        },
        /** @param {number} [since] */
        async events(since = 0) {
            const r = await __fyloFetch(`${basePath}/api/events?collection=${encodeURIComponent(collection)}&since=${since}`);
            return r.json();
        },
        /** @param {string} id @param {Record<string, unknown>} doc */
        async patch(id, doc) {
            return __fyloPostJson(basePath, '/api/patch', { collection, id, doc });
        },
        /** @param {string} id */
        async del(id) {
            const r = await __fyloFetch(`${basePath}/api/delete?collection=${encodeURIComponent(collection)}&id=${encodeURIComponent(id)}`, { method: 'DELETE' });
            return r.json();
        },
        async rebuild() {
            return __fyloPostJson(basePath, '/api/rebuild', { collection });
        },
    };
}

/**
 * @param {string} basePath
 * @returns {{ state: Record<string, unknown>, proxy: any }}
 */
function createFyloClient(basePath) {
    const state = {
        enabled: false,
        /** @type {string | undefined} */
        root: undefined,
        /**
         * @param {string} user
         * @param {string} pass
         */
        setCredentials(user, pass) {
            __fyloAuthHeader = __fyloBasicAuth(user, pass);
        },
        clearCredentials() {
            __fyloAuthHeader = null;
        },
        /** @param {string} source */
        sql(source) {
            return __fyloPostJson(basePath, '/api/query', { kind: 'sql', source });
        },
        async collections() {
            const r = await __fyloFetch(`${basePath}/api/collections`, { cache: 'reload' });
            if (!r.ok) return { root: '', collections: [] };
            const d = await r.json();
            state.root = d.root;
            return d;
        },
        async meta() {
            const r = await __fyloFetch(`${basePath}/api/meta`, { cache: 'reload' });
            if (!r.ok) return null;
            return r.json();
        },
    };

    const proxy = /** @type {any} */ (new Proxy(state, {
        get(target, prop) {
            if (typeof prop !== 'string') return Reflect.get(target, prop);
            if (prop in target) return Reflect.get(target, prop);
            return __fyloCollection(basePath, prop);
        },
        set(target, prop, value) {
            return Reflect.set(target, prop, value);
        },
        has(target, prop) {
            return typeof prop === 'string' || prop in target;
        },
    }));

    return { state, proxy };
}

// ── Noop fallback (server-side / prerender) ────────────────────────────────
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
    /**
     * @param {string} user
     * @param {string} pass
     */
    setCredentials(user, pass) {},
    clearCredentials() {},
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

// ── Bootstrap window.fylo ──────────────────────────────────────────────────
// If running in a browser context and no prior bootstrap has set window.fylo,
// create the real Fylo client from the shell-injected meta tag.
if (typeof window !== 'undefined'
    && !/** @type {any} */ (window).fylo
    && !/** @type {Record<string, unknown>} */ (globalThis).__ty_prerender__) {
    const basePath = resolveBrowserPath();
    const { state, proxy } = createFyloClient(basePath);
    /** @type {any} */ (window).fylo = proxy;

    // Probe meta once; if the browser route is mounted, flip enabled and cache the root.
    __fyloFetch(`${basePath}/api/meta`, { cache: 'reload' })
        .then(r => r.ok ? r.json() : null)
        .then(meta => {
            if (meta) {
                state.enabled = true;
                state.root = meta.root;
            }
        })
        .catch(() => { /* fylo browser not mounted; leave disabled */ });
}

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
