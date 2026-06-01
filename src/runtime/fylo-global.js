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

import { tacBrowserCache } from './browser-cache.js';

/**
 * @typedef {'cache-first' | 'network-first' | 'reload' | 'no-store'} FyloCachePolicy
 * @typedef {{ cache?: FyloCachePolicy }} FyloQueryOptions
 * @typedef {{ collection?: string, events?: unknown[], offset?: number, exists?: boolean, error?: string }} FyloEventsPayload
 * @typedef {'initial' | 'event-stream' | 'poll'} FyloSubscribeSource
 * @typedef {{ collection: string, events: unknown[], offset: number, source: FyloSubscribeSource }} FyloSubscribeMeta
 * @typedef {(payload: unknown, meta: FyloSubscribeMeta) => void | Promise<void>} FyloSubscribeCallback
 * @typedef {FyloQueryOptions & { pollMs?: number, since?: number, onError?: (error: unknown) => void }} FyloSubscribeOptions
 */

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

/** @param {string} value */
function hashString(value) {
    let hash = 0x811c9dc5;
    for (let i = 0; i < value.length; i += 1) {
        hash ^= value.charCodeAt(i);
        hash = Math.imul(hash, 0x01000193);
    }
    return (hash >>> 0).toString(36);
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
     * @param {FyloQueryOptions} [options]
     */
    async find(query = {}, options = {}) {
        const params = new URLSearchParams();
        for (const [key, value] of Object.entries(query).sort(([a], [b]) => a.localeCompare(b))) {
            params.set(key, String(value));
        }
        const qs = params.toString();
        const url = `/${encodeURIComponent(this.collection)}/${qs ? `?${qs}` : ''}`;
        const response = await this.browserClient.fetch(url, {}, {
            cache: options.cache,
            cacheKey: this.browserClient.cacheKey('GET', url),
        });
        return response.json();
    }

    /** @param {number} [limit] @param {FyloQueryOptions} [options] */
    async list(limit = 25, options = {}) {
        const url = `/${encodeURIComponent(this.collection)}/?limit=${limit}`;
        const response = await this.browserClient.fetch(url, {}, {
            cache: options.cache,
            cacheKey: this.browserClient.cacheKey('GET', url),
        });
        return response.json();
    }

    /** @param {string} id @param {FyloQueryOptions} [options] */
    async get(id, options = {}) {
        const url = `/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`;
        const response = await this.browserClient.fetch(url, {}, {
            cache: options.cache,
            cacheKey: this.browserClient.cacheKey('GET', url),
        });
        return response.json();
    }

    /** @param {number | 'latest'} [since] */
    async events(since = 0) {
        const response = await this.browserClient.fetch(`/api/events?collection=${encodeURIComponent(this.collection)}&since=${since}`, {}, {
            cache: 'no-store',
        });
        return response.json();
    }

    /**
     * Subscribe to collection changes and re-run a cached FYLO query whenever
     * the FYLO event journal changes.
     * @param {Record<string, unknown> | FyloSubscribeCallback} [queryOrCallback]
     * @param {FyloSubscribeCallback | FyloSubscribeOptions} [callbackOrOptions]
     * @param {FyloSubscribeOptions} [maybeOptions]
     * @returns {() => void}
     */
    subscribe(queryOrCallback = {}, callbackOrOptions = {}, maybeOptions = {}) {
        if (typeof queryOrCallback === 'function') {
            return this.browserClient.subscribeCollection(
                this.collection,
                {},
                queryOrCallback,
                /** @type {FyloSubscribeOptions} */ (callbackOrOptions ?? {}),
            );
        }
        if (typeof callbackOrOptions !== 'function') {
            throw new TypeError('fylo.<collection>.subscribe(query, callback, options) requires a callback');
        }
        return this.browserClient.subscribeCollection(
            this.collection,
            queryOrCallback,
            callbackOrOptions,
            maybeOptions,
        );
    }

    /** @param {Record<string, unknown>} doc */
    async create(doc) {
        return this.browserClient.postJson(`/${encodeURIComponent(this.collection)}/`, doc, this.collection);
    }

    /** @param {string} id @param {Record<string, unknown>} doc */
    async put(id, doc) {
        const response = await this.browserClient.fetch(`/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(doc),
        }, {
            invalidatePrefixes: [this.browserClient.collectionCachePrefix(this.collection)],
            invalidateKeys: [this.browserClient.cacheKey('GET', `/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`)],
        });
        return response.json();
    }

    /** @param {string} id @param {Record<string, unknown>} doc */
    async patch(id, doc) {
        const response = await this.browserClient.fetch(`/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`, {
            method: 'PATCH',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(doc),
        }, {
            invalidatePrefixes: [this.browserClient.collectionCachePrefix(this.collection)],
            invalidateKeys: [this.browserClient.cacheKey('GET', `/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`)],
        });
        return response.json();
    }

    /** @param {string} id */
    async del(id) {
        const response = await this.browserClient.fetch(`/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`, { method: 'DELETE' }, {
            invalidatePrefixes: [this.browserClient.collectionCachePrefix(this.collection)],
            invalidateKeys: [this.browserClient.cacheKey('GET', `/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`)],
        });
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

    /** @returns {string} */
    authScope() {
        return this.authHeader ? `auth-${hashString(this.authHeader)}` : 'anon';
    }

    /** @param {string} path */
    normalizedPath(path) {
        return path.startsWith('/') ? path : `/${path}`;
    }

    /**
     * @param {string} apiPath
     * @returns {string}
     */
    resolveUrl(apiPath) {
        const baseUrl = typeof window !== 'undefined' && window.location?.href && window.location.href !== 'about:blank'
            ? window.location.href
            : 'http://localhost/';
        return new URL(`${this.basePath}${apiPath}`, baseUrl).href;
    }

    /**
     * @param {string} method
     * @param {string} apiPath
     * @returns {string}
     */
    cacheKey(method, apiPath) {
        return `fylo:${this.authScope()}:${this.basePath}:${method.toUpperCase()}:${this.normalizedPath(apiPath)}`;
    }

    /**
     * @param {string} collection
     * @returns {string}
     */
    collectionCachePrefix(collection) {
        return `fylo:${this.authScope()}:${this.basePath}:GET:/${encodeURIComponent(collection)}/`;
    }

    /**
     * @param {string} collection
     * @returns {Promise<void>}
     */
    invalidateCollection(collection) {
        return tacBrowserCache.invalidate([], [this.collectionCachePrefix(collection)]);
    }

    /**
     * @param {string} apiPath
     * @param {RequestInit} [init]
     * @param {FyloQueryOptions & { cacheKey?: string | null, invalidateKeys?: string[], invalidatePrefixes?: string[] }} [options]
     * @returns {Promise<Response>}
     */
    fetch(apiPath, init = {}, options = {}) {
        const headers = new Headers(init.headers || {});
        if (this.authHeader) headers.set('Authorization', this.authHeader);
        const requestInit = { ...init, headers };
        const request = new Request(this.resolveUrl(apiPath), requestInit);
        const method = request.method.toUpperCase();
        return tacBrowserCache.fetch(request, undefined, {
            cache: options.cache,
            key: options.cacheKey ?? ((method === 'GET' || method === 'HEAD') ? this.cacheKey(method, apiPath) : null),
            invalidateKeys: options.invalidateKeys,
            invalidatePrefixes: options.invalidatePrefixes,
        });
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
            return tacBrowserCache.fetch(apiPath, { ...init, headers }, { cache: 'network-first' });
        }
        const path = apiPath.startsWith(this.basePath) ? apiPath.slice(this.basePath.length) || '/' : apiPath;
        return this.fetch(path, init);
    }

    /**
     * @param {string} apiPath
     * @param {Record<string, unknown>} body
     * @param {string} [collection]
     * @returns {Promise<any>}
     */
    async postJson(apiPath, body, collection) {
        const response = await this.fetch(apiPath, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body),
        }, {
            invalidatePrefixes: collection ? [this.collectionCachePrefix(collection)] : [],
        });
        return response.json();
    }

    /**
     * @param {number | undefined} value
     * @returns {number}
     */
    normalizePollMs(value) {
        if (!Number.isFinite(value) || !value || value <= 0) return 1000;
        return Math.max(250, Math.min(Math.trunc(value), 60000));
    }

    /**
     * @param {string} collection
     * @param {Record<string, unknown>} query
     * @param {FyloSubscribeCallback} callback
     * @param {FyloSubscribeOptions} [options]
     * @returns {() => void}
     */
    subscribeCollection(collection, query, callback, options = {}) {
        const client = new FyloCollectionClient(this, collection);
        const pollMs = this.normalizePollMs(options.pollMs);
        const hasExplicitSince = Number.isFinite(options.since) && Number(options.since) >= 0;
        let offset = hasExplicitSince
            ? Math.trunc(Number(options.since))
            : 0;
        const startingOffset = hasExplicitSince ? String(offset) : 'latest';
        let active = true;
        let refreshing = false;
        /** @type {unknown[]} */
        let pendingEvents = [];
        /** @type {FyloSubscribeSource | null} */
        let pendingSource = null;
        /** @type {EventSource | null} */
        let eventSource = null;
        /** @type {ReturnType<typeof setInterval> | null} */
        let pollTimer = null;

        /** @param {unknown} error */
        const reportError = (error) => {
            if (typeof options.onError === 'function') options.onError(error);
        };

        /** @param {FyloSubscribeSource} source @param {unknown[]} events */
        const refresh = async (source, events = []) => {
            if (!active) return;
            if (refreshing) {
                if (source !== 'initial') {
                    pendingEvents = pendingEvents.concat(events);
                    pendingSource = source;
                }
                return;
            }
            refreshing = true;
            try {
                if (source !== 'initial') await this.invalidateCollection(collection);
                const result = await client.find(query, {
                    cache: source === 'initial' ? (options.cache ?? 'network-first') : 'reload',
                });
                if (active) {
                    await callback(result, { collection, events, offset, source });
                }
            } catch (error) {
                reportError(error);
            } finally {
                refreshing = false;
                if (active && pendingEvents.length > 0) {
                    const nextEvents = pendingEvents;
                    const nextSource = pendingSource ?? 'event-stream';
                    pendingEvents = [];
                    pendingSource = null;
                    await refresh(nextSource, nextEvents);
                }
            }
        };

        /** @param {FyloEventsPayload} payload @param {FyloSubscribeSource} source */
        const handleEventsPayload = async (payload, source) => {
            if (!active) return;
            if (payload.error) {
                reportError(new Error(payload.error));
                return;
            }
            offset = Number.isFinite(payload.offset) && Number(payload.offset) >= 0
                ? Math.trunc(Number(payload.offset))
                : offset;
            const events = Array.isArray(payload.events) ? payload.events : [];
            if (events.length > 0) await refresh(source, events);
        };

        refresh('initial');

        const canUseEventSource = typeof EventSource !== 'undefined' && !this.authHeader;
        if (canUseEventSource) {
            const streamUrl = new URL(this.resolveUrl('/api/events/stream'));
            streamUrl.searchParams.set('collection', collection);
            streamUrl.searchParams.set('since', startingOffset);
            streamUrl.searchParams.set('poll', String(pollMs));
            eventSource = new EventSource(streamUrl.href);
            eventSource.addEventListener('fylo.events', (event) => {
                try {
                    handleEventsPayload(JSON.parse(/** @type {MessageEvent} */ (event).data), 'event-stream');
                } catch (error) {
                    reportError(error);
                }
            });
            eventSource.addEventListener('fylo.error', (event) => {
                try {
                    const payload = JSON.parse(/** @type {MessageEvent} */ (event).data);
                    reportError(new Error(payload.error || 'FYLO subscription stream failed'));
                } catch (error) {
                    reportError(error);
                }
            });
            eventSource.onerror = () => {
                reportError(new Error('FYLO subscription stream interrupted; the browser will retry automatically'));
            };
        } else {
            const poll = async () => {
                try {
                    const payload = /** @type {FyloEventsPayload} */ (await client.events(offset === 0 && !hasExplicitSince ? 'latest' : offset));
                    await handleEventsPayload(payload, 'poll');
                } catch (error) {
                    reportError(error);
                }
            };
            pollTimer = setInterval(poll, pollMs);
        }

        return () => {
            active = false;
            if (eventSource) eventSource.close();
            if (pollTimer) clearInterval(pollTimer);
        };
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
export function createFyloClient(basePath) {
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
    /** @returns {() => void} */
    subscribe() { return () => {}; },
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
