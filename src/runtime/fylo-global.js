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

class FyloCollectionClient {
    /**
     * @param {FyloBrowserClient} browserClient
     * @param {string} collection
     */
    constructor(browserClient, collection) {
        this.browserClient = browserClient;
        this.collection = collection;
    }

    /**
     * Query the collection using PostgREST-style filters.
     * e.g. find({ role: 'eq.admin', age: 'gt.18', select: 'name,role', order: 'name.asc', limit: 10 })
     * @param {Record<string, unknown>} [query]
     */
    async find(query = {}) {
        const params = new URLSearchParams();
        for (const [key, value] of Object.entries(query)) {
            params.set(key, String(value));
        }
        const qs = params.toString();
        const url = `/${encodeURIComponent(this.collection)}/${qs ? `?${qs}` : ''}`;
        const response = await this.browserClient.fetch(url);
        return response.json();
    }

    /** @param {number} [limit] */
    async list(limit = 25) {
        const response = await this.browserClient.fetch(`/${encodeURIComponent(this.collection)}/?limit=${limit}`);
        return response.json();
    }

    /** @param {string} id */
    async get(id) {
        const response = await this.browserClient.fetch(`/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`);
        return response.json();
    }

    /** @param {number} [since] */
    async events(since = 0) {
        const response = await this.browserClient.fetch(`/api/events?collection=${encodeURIComponent(this.collection)}&since=${since}`);
        return response.json();
    }

    /** @param {Record<string, unknown>} doc */
    async create(doc) {
        return this.browserClient.postJson(`/${encodeURIComponent(this.collection)}/`, doc);
    }

    /** @param {string} id @param {Record<string, unknown>} doc */
    async put(id, doc) {
        const response = await this.browserClient.fetch(`/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(doc),
        });
        return response.json();
    }

    /** @param {string} id @param {Record<string, unknown>} doc */
    async patch(id, doc) {
        const response = await this.browserClient.fetch(`/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`, {
            method: 'PATCH',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(doc),
        });
        return response.json();
    }

    /** @param {string} id */
    async del(id) {
        const response = await this.browserClient.fetch(`/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`, { method: 'DELETE' });
        if (response.status === 204) return { ok: true };
        return response.json();
    }

    async rebuild() {
        return this.browserClient.postJson('/api/rebuild', { collection: this.collection });
    }
}

class FyloBrowserClient {
    /** @param {string} basePath */
    constructor(basePath) {
        this.basePath = basePath;
        /** @type {string | null} */
        this.authHeader = null;
        this.state = this.createState();
        this.proxy = this.createProxy();
    }

    /**
     * Encodes Basic auth credentials as UTF-8 before Base64 conversion.
     * @param {string} user
     * @param {string} pass
     * @returns {string}
     */
    basicAuth(user, pass) {
        const bytes = new TextEncoder().encode(`${user}:${pass}`);
        let binary = '';
        for (const byte of bytes) binary += String.fromCharCode(byte);
        return `Basic ${btoa(binary)}`;
    }

    /** @returns {Record<string, unknown>} */
    createState() {
        return {
            enabled: false,
            /** @type {string | undefined} */
            root: undefined,
            setCredentials: this.setCredentials.bind(this),
            clearCredentials: this.clearCredentials.bind(this),
            collections: this.collections.bind(this),
            meta: this.meta.bind(this),
            request: this.request.bind(this),
        };
    }

    createProxy() {
        return /** @type {any} */ (new Proxy(this.state, {
            get: (target, prop) => {
                if (typeof prop !== 'string') return Reflect.get(target, prop);
                if (prop in target) return Reflect.get(target, prop);
                return new FyloCollectionClient(this, prop);
            },
            set(target, prop, value) {
                return Reflect.set(target, prop, value);
            },
            has(target, prop) {
                return typeof prop === 'string' || prop in target;
            },
        }));
    }

    /**
     * @param {string} user
     * @param {string} pass
     */
    setCredentials(user, pass) {
        this.authHeader = this.basicAuth(user, pass);
    }

    clearCredentials() {
        this.authHeader = null;
    }

    /**
     * @param {string} apiPath
     * @param {RequestInit} [init]
     * @returns {Promise<Response>}
     */
    fetch(apiPath, init = {}) {
        const headers = new Headers(init.headers || {});
        if (this.authHeader) headers.set('Authorization', this.authHeader);
        return fetch(`${this.basePath}${apiPath}`, { ...init, headers });
    }

    /**
     * @param {string} apiPath
     * @param {RequestInit} [init]
     * @returns {Promise<Response>}
     */
    request(apiPath, init = {}) {
        if (/^https?:\/\//i.test(apiPath)) {
            const headers = new Headers(init.headers || {});
            if (this.authHeader) headers.set('Authorization', this.authHeader);
            return fetch(apiPath, { ...init, headers });
        }
        const path = apiPath.startsWith(this.basePath) ? apiPath.slice(this.basePath.length) || '/' : apiPath;
        return this.fetch(path, init);
    }

    /**
     * @param {string} apiPath
     * @param {Record<string, unknown>} body
     * @returns {Promise<any>}
     */
    async postJson(apiPath, body) {
        const response = await this.fetch(apiPath, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body),
        });
        return response.json();
    }

    async collections() {
        const response = await this.fetch('/api/collections', { cache: 'reload' });
        if (!response.ok) return { root: '', collections: [] };
        const collectionsPayload = await response.json();
        this.state.root = collectionsPayload.root;
        return collectionsPayload;
    }

    async meta() {
        const response = await this.fetch('/api/meta', { cache: 'reload' });
        if (!response.ok) return null;
        return response.json();
    }

    async probe() {
        try {
            const meta = await this.meta();
            if (!meta)
                return;
            this.state.enabled = true;
            this.state.root = meta.root;
        } catch {
            /* fylo browser not mounted; leave disabled */
        }
    }
}

/** @param {string} basePath */
function createFyloClient(basePath) {
    return new FyloBrowserClient(basePath);
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
    async create() { return { error: 'Fylo browser not enabled' }; },
    /** @returns {Promise<{ error: string }>} */
    async put() { return { error: 'Fylo browser not enabled' }; },
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
    /** @returns {Promise<{ root: string, collections: unknown[] }>} */
    async collections() { return { root: '', collections: [] }; },
    /** @returns {Promise<null>} */
    async meta() { return null; },
    /** @returns {Promise<Response>} */
    async request() { return new Response(JSON.stringify({ error: 'Fylo browser not enabled' }), { status: 503 }); },
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
    const client = createFyloClient(basePath);
    /** @type {any} */ (window).fylo = client.proxy;
    client.probe();
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
