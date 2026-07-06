// @ts-check

/**
 * Browser-local Fylo document engine for Tac.
 *
 * OPFS is preferred because it gives Tachyon a real browser-side document store
 * instead of only caching HTTP responses. When OPFS is unavailable, the engine
 * falls back to an in-memory store so the public API still behaves consistently
 * during tests and older browser sessions.
 *
 * @typedef {'cache-first' | 'network-first' | 'reload' | 'no-store'} FyloLocalCachePolicy
 * @typedef {{ cache?: FyloLocalCachePolicy }} FyloLocalQueryOptions
 * @typedef {{ collection: string, docs: Array<{ id: string, doc: unknown }>, encryptedFields?: string[], revealed?: boolean, local?: boolean, source?: string }} FyloLocalFindResult
 * @typedef {{ collection: string, id: string, doc: unknown, encryptedFields?: string[], revealed?: boolean, local?: boolean, source?: string }} FyloLocalGetResult
 * @typedef {{ id: string, doc: unknown, op: 'snapshot' | 'create' | 'put' | 'patch' | 'delete', time: number }} FyloLocalEvent
 * @typedef {{ version: 1, docs: Record<string, unknown>, events: FyloLocalEvent[], offset: number, updatedAt: number }} FyloLocalCollectionFile
 * @typedef {{ available(): Promise<boolean>, find(scope: string, collection: string, query?: Record<string, unknown>): Promise<FyloLocalFindResult>, get(scope: string, collection: string, id: string): Promise<FyloLocalGetResult | null>, ingestFindResult(scope: string, collection: string, payload: unknown): Promise<void>, ingestGetResult(scope: string, collection: string, payload: unknown): Promise<void>, put(scope: string, collection: string, id: string, doc: unknown, op?: 'snapshot' | 'create' | 'put' | 'patch'): Promise<void>, delete(scope: string, collection: string, id: string): Promise<void>, subscribe(scope: string, collection: string, listener: () => void): () => void }} FyloLocalCoordinator
 */

const FILE_VERSION = 1;
const MAX_EVENTS = 500;
const RESERVED_PARAMS = new Set(['collection', 'limit', 'offset', 'select', 'order']);
const POSTGREST_OPS = new Set(['eq', 'neq', 'gt', 'gte', 'lt', 'lte', 'like', 'ilike', 'in', 'is']);

/** @param {string} value */
function safeSegment(value) {
    return encodeURIComponent(value).replace(/%/g, '_');
}

/** @returns {boolean} */
function isBrowser() {
    return typeof navigator !== 'undefined'
        && (typeof window !== 'undefined' || typeof globalThis.postMessage === 'function')
        && !/** @type {Record<string, unknown>} */ (globalThis).__tc_prerender__;
}

/**
 * Synchronous OPFS access handles (`createSyncAccessHandle`) provide fast,
 * in-place, durable byte writes, but browsers only permit them inside a Worker
 * (never on the UI thread, where sync file I/O would freeze the screen). We
 * detect a Worker context by the absence of `window`; callers still
 * feature-detect `createSyncAccessHandle` on the handle before using it.
 * @returns {boolean}
 */
function canUseSyncAccessHandle() {
    return typeof window === 'undefined' && typeof self !== 'undefined';
}

/** @param {unknown} value */
function cloneJson(value) {
    if (value === undefined)
        return undefined;
    return JSON.parse(JSON.stringify(value));
}

/** @param {unknown} payload */
function docsFromPayload(payload) {
    if (!payload || typeof payload !== 'object')
        return null;
    const docs = /** @type {{ docs?: unknown }} */ (payload).docs;
    if (!Array.isArray(docs))
        return null;
    /** @type {Array<{ id: string, doc: unknown }>} */
    const out = [];
    for (const entry of docs) {
        if (!entry || typeof entry !== 'object')
            continue;
        const record = /** @type {{ id?: unknown, doc?: unknown }} */ (entry);
        if (typeof record.id !== 'string')
            continue;
        out.push({ id: record.id, doc: cloneJson(record.doc) });
    }
    return out;
}

/** @param {unknown} payload */
function docFromPayload(payload) {
    if (!payload || typeof payload !== 'object')
        return null;
    const record = /** @type {{ id?: unknown, doc?: unknown }} */ (payload);
    if (typeof record.id !== 'string')
        return null;
    return { id: record.id, doc: cloneJson(record.doc) };
}

/**
 * @param {string} raw
 * @returns {{ [op: string]: unknown } | null}
 */
function parsePostgrestValue(raw) {
    const dotIndex = raw.indexOf('.');
    if (dotIndex === -1)
        return null;
    const head = raw.slice(0, dotIndex);
    const tail = raw.slice(dotIndex + 1);
    if (head === 'not') {
        const inner = parsePostgrestValue(tail);
        if (!inner)
            return null;
        /** @type {Record<string, string>} */
        const inverses = { $eq: '$neq', $neq: '$eq', $lt: '$gte', $lte: '$gt', $gt: '$lte', $gte: '$lt' };
        const [[op, val]] = Object.entries(inner);
        const inverse = inverses[op];
        return inverse ? { [inverse]: val } : { $not: inner };
    }
    if (!POSTGREST_OPS.has(head))
        return null;
    if (head === 'in') {
        const match = tail.match(/^\((.+)\)$/);
        if (!match)
            return null;
        const values = [];
        let current = '';
        let inQuote = false;
        for (const ch of match[1]) {
            if (ch === '"') { inQuote = !inQuote; continue; }
            if (ch === ',' && !inQuote) { values.push(current.trim()); current = ''; continue; }
            current += ch;
        }
        if (current.trim())
            values.push(current.trim());
        return { $in: values };
    }
    if (head === 'is') {
        if (tail === 'null') return { $is: null };
        if (tail === 'true') return { $is: true };
        if (tail === 'false') return { $is: false };
        return null;
    }
    if (head === 'like' || head === 'ilike')
        return { [`$${head}`]: tail.replace(/\*/g, '%') };
    return { [`$${head}`]: tail };
}

/** @param {string} raw */
function parseOrder(raw) {
    return raw.split(',').map((segment) => {
        const trimmed = segment.trim();
        if (!trimmed)
            return null;
        const parts = trimmed.split('.');
        const field = parts[0];
        const desc = parts.includes('desc');
        return field ? { field, desc } : null;
    }).filter(/** @returns {entry is { field: string, desc: boolean }} */ (entry) => entry !== null);
}

/**
 * @param {unknown} doc
 * @param {string[] | null} fields
 */
function selectFields(doc, fields) {
    if (!fields || !doc || typeof doc !== 'object' || Array.isArray(doc))
        return doc;
    /** @type {Record<string, unknown>} */
    const out = {};
    for (const field of fields) {
        if (field in /** @type {Record<string, unknown>} */ (doc))
            out[field] = /** @type {Record<string, unknown>} */ (doc)[field];
    }
    return out;
}

/**
 * @param {unknown} doc
 * @param {Array<{ field: string, filter: Record<string, unknown> }>} filters
 */
function matchesAllFilters(doc, filters) {
    if (!doc || typeof doc !== 'object')
        return false;
    const record = /** @type {Record<string, unknown>} */ (doc);
    for (const { field, filter } of filters) {
        const value = record[field];
        for (const [op, expected] of Object.entries(filter)) {
            switch (op) {
                case '$eq': if (String(value) !== String(expected)) return false; break;
                case '$neq': if (String(value) === String(expected)) return false; break;
                case '$gt': if (!(String(value) > String(expected))) return false; break;
                case '$gte': if (!(String(value) >= String(expected))) return false; break;
                case '$lt': if (!(String(value) < String(expected))) return false; break;
                case '$lte': if (!(String(value) <= String(expected))) return false; break;
                case '$in': if (!Array.isArray(expected) || !expected.some((v) => String(value) === String(v))) return false; break;
                case '$is': {
                    if (expected === null && value != null) return false;
                    if (expected === true && value !== true) return false;
                    if (expected === false && value !== false) return false;
                    break;
                }
                case '$like': {
                    const escaped = String(expected).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
                    const pattern = escaped.replace(/%/g, '.*');
                    if (!new RegExp(`^${pattern}$`).test(String(value ?? ''))) return false;
                    break;
                }
                case '$ilike': {
                    const escaped = String(expected).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
                    const pattern = escaped.replace(/%/g, '.*');
                    if (!new RegExp(`^${pattern}$`, 'i').test(String(value ?? ''))) return false;
                    break;
                }
                default: break;
            }
        }
    }
    return true;
}

/** @param {Record<string, unknown>} query */
function parseQuery(query) {
    const limitParam = Number(query.limit ?? 25);
    const limit = Number.isFinite(limitParam) && limitParam > 0 ? Math.min(Math.trunc(limitParam), 200) : 25;
    const offsetParam = Number(query.offset ?? 0);
    const offset = Number.isFinite(offsetParam) && offsetParam >= 0 ? Math.trunc(offsetParam) : 0;
    const select = typeof query.select === 'string'
        ? query.select.split(',').map((s) => s.trim()).filter(Boolean)
        : null;
    const order = typeof query.order === 'string' ? parseOrder(query.order) : null;
    /** @type {Array<{ field: string, filter: Record<string, unknown> }>} */
    const filters = [];
    for (const [key, value] of Object.entries(query)) {
        if (RESERVED_PARAMS.has(key))
            continue;
        const filter = parsePostgrestValue(String(value));
        if (filter)
            filters.push({ field: key, filter });
    }
    return { limit, offset, select, order, filters };
}

class FyloMemoryLocalStore {
    constructor() {
        /** @type {Map<string, FyloLocalCollectionFile>} */
        this.collections = new Map();
    }

    /** @param {string} key */
    async read(key) {
        return cloneJson(this.collections.get(key) ?? null);
    }

    /**
     * @param {string} key
     * @param {FyloLocalCollectionFile} value
     */
    async write(key, value) {
        this.collections.set(key, cloneJson(value));
    }
}

/**
 * The OPFS synchronous access handle (not yet in lib.dom across all toolchains).
 * Every method is synchronous; only `createSyncAccessHandle()` is async, and it
 * resolves exclusively inside a Worker.
 * @typedef {{
 *   getSize(): number,
 *   read(buffer: ArrayBufferView, options?: { at?: number }): number,
 *   write(buffer: ArrayBufferView, options?: { at?: number }): number,
 *   truncate(size: number): void,
 *   flush(): void,
 *   close(): void,
 * }} FyloSyncAccessHandle
 */

class FyloOpfsLocalStore {
    /** @param {FileSystemDirectoryHandle} root */
    constructor(root) {
        this.root = root;
        this.encoder = new TextEncoder();
        this.decoder = new TextDecoder();
        // Prefer the synchronous, in-place, durable access handle when we run in
        // a Worker; only fall back to the slower async writable on the UI thread.
        this.useSync = canUseSyncAccessHandle();
    }

    /** @param {string} key */
    async fileHandle(key) {
        const dir = await this.root.getDirectoryHandle('tachyon-fylo', { create: true });
        return dir.getFileHandle(`${safeSegment(key)}.json`, { create: true });
    }

    /**
     * Open a synchronous access handle if the engine should and can use one.
     * @param {FileSystemFileHandle} handle
     * @returns {Promise<FyloSyncAccessHandle | null>}
     */
    async openSync(handle) {
        const candidate = /** @type {{ createSyncAccessHandle?: () => Promise<FyloSyncAccessHandle> }} */ (handle);
        if (this.useSync && typeof candidate.createSyncAccessHandle === 'function')
            return candidate.createSyncAccessHandle();
        return null;
    }

    /** @param {string} key */
    async read(key) {
        try {
            const handle = await this.fileHandle(key);
            const access = await this.openSync(handle);
            if (access) {
                try {
                    const size = access.getSize();
                    if (size === 0)
                        return null;
                    const buffer = new Uint8Array(size);
                    access.read(buffer, { at: 0 });
                    const text = this.decoder.decode(buffer);
                    return text.trim() ? JSON.parse(text) : null;
                } finally {
                    access.close();
                }
            }
            const text = await (await handle.getFile()).text();
            return text.trim() ? JSON.parse(text) : null;
        } catch {
            return null;
        }
    }

    /**
     * @param {string} key
     * @param {FyloLocalCollectionFile} value
     */
    async write(key, value) {
        const handle = await this.fileHandle(key);
        const json = JSON.stringify(value);
        const access = await this.openSync(handle);
        if (access) {
            try {
                // In-place overwrite, then flush so the bytes reach disk
                // immediately — durable even if the tab closes unexpectedly.
                access.truncate(0);
                access.write(this.encoder.encode(json), { at: 0 });
                access.flush();
            } finally {
                access.close();
            }
            return;
        }
        const writable = await handle.createWritable();
        await writable.write(json);
        await writable.close();
    }
}

export class FyloLocalEngine {
    /** @param {{ store?: FyloMemoryLocalStore | FyloOpfsLocalStore }} [options] */
    constructor(options = {}) {
        this.store = options.store ?? null;
        /** @type {Promise<FyloMemoryLocalStore | FyloOpfsLocalStore | null> | null} */
        this.storePromise = null;
        /** @type {Map<string, Promise<unknown>>} */
        this.queues = new Map();
        /** @type {Map<string, Set<() => void>>} */
        this.listeners = new Map();
    }

    /** @returns {Promise<FyloMemoryLocalStore | FyloOpfsLocalStore | null>} */
    async openStore() {
        if (!isBrowser())
            return null;
        if (this.store)
            return this.store;
        if (this.storePromise)
            return this.storePromise;
        this.storePromise = (async () => {
            const storage = /** @type {{ storage?: { getDirectory?: () => Promise<FileSystemDirectoryHandle> } }} */ (navigator).storage;
            if (storage?.getDirectory) {
                try {
                    return new FyloOpfsLocalStore(await storage.getDirectory());
                } catch {
                    // Fall through to memory. OPFS may be denied in private mode.
                }
            }
            return getMemoryStore();
        })();
        this.store = await this.storePromise;
        return this.store;
    }

    /** @returns {Promise<boolean>} */
    async available() {
        return Boolean(await this.openStore());
    }

    /**
     * @param {string} scope
     * @param {string} collection
     */
    collectionKey(scope, collection) {
        return `${scope}:${collection}`;
    }

    /**
     * @param {string} scope
     * @param {string} collection
     */
    async readCollection(scope, collection) {
        const store = await this.openStore();
        if (!store)
            return emptyCollection();
        const file = await store.read(this.collectionKey(scope, collection));
        return normalizeCollectionFile(file);
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {FyloLocalCollectionFile} file
     */
    async writeCollection(scope, collection, file) {
        const store = await this.openStore();
        if (!store)
            return;
        await store.write(this.collectionKey(scope, collection), normalizeCollectionFile(file));
    }

    /**
     * @template T
     * @param {string} scope
     * @param {string} collection
     * @param {(file: FyloLocalCollectionFile) => T | Promise<T>} op
     * @returns {Promise<T>}
     */
    enqueue(scope, collection, op) {
        const key = this.collectionKey(scope, collection);
        const previous = this.queues.get(key) ?? Promise.resolve();
        const next = previous.catch(() => undefined).then(async () => {
            const file = await this.readCollection(scope, collection);
            const result = await op(file);
            await this.writeCollection(scope, collection, file);
            return result;
        });
        this.queues.set(key, next.finally(() => {
            if (this.queues.get(key) === next)
                this.queues.delete(key);
        }));
        return next;
    }

    /**
     * Serialize a read with the per-collection write queue. Synchronous OPFS
     * access handles are exclusive, so only one handle may be open for a file at
     * a time; unlike `enqueue`, this does not write the file back.
     * @template T
     * @param {string} scope
     * @param {string} collection
     * @param {(file: FyloLocalCollectionFile) => T} op
     * @returns {Promise<T>}
     */
    enqueueRead(scope, collection, op) {
        const key = this.collectionKey(scope, collection);
        const previous = this.queues.get(key) ?? Promise.resolve();
        const next = previous.catch(() => undefined).then(async () => op(await this.readCollection(scope, collection)));
        this.queues.set(key, next.finally(() => {
            if (this.queues.get(key) === next)
                this.queues.delete(key);
        }));
        return next;
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {Record<string, unknown>} query
     */
    async find(scope, collection, query = {}) {
        return this.enqueueRead(scope, collection, (file) => {
            const { limit, offset, select, order, filters } = parseQuery(query);
            /** @type {Array<{ id: string, doc: unknown }>} */
            let docs = Object.entries(file.docs).map(([id, doc]) => ({ id, doc: cloneJson(doc) }));
            if (filters.length > 0)
                docs = docs.filter((entry) => matchesAllFilters(entry.doc, filters));
            if (order?.length) {
                docs.sort((a, b) => {
                    for (const { field, desc } of order) {
                        const aVal = /** @type {Record<string, unknown>} */ (a.doc)?.[field];
                        const bVal = /** @type {Record<string, unknown>} */ (b.doc)?.[field];
                        if (aVal === bVal) continue;
                        if (aVal == null) return desc ? -1 : 1;
                        if (bVal == null) return desc ? 1 : -1;
                        const cmp = aVal < bVal ? -1 : 1;
                        return desc ? -cmp : cmp;
                    }
                    return 0;
                });
            }
            docs = docs.slice(offset, offset + limit).map((entry) => ({
                id: entry.id,
                doc: selectFields(entry.doc, select),
            }));
            return { collection, docs, local: true, source: this.store instanceof FyloOpfsLocalStore ? 'opfs' : 'memory' };
        });
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {string} id
     */
    async get(scope, collection, id) {
        return this.enqueueRead(scope, collection, (file) => {
            if (!(id in file.docs))
                return null;
            return { collection, id, doc: cloneJson(file.docs[id]), local: true, source: this.store instanceof FyloOpfsLocalStore ? 'opfs' : 'memory' };
        });
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {unknown} payload
     */
    async ingestFindResult(scope, collection, payload) {
        const docs = docsFromPayload(payload);
        if (!docs)
            return;
        await this.enqueue(scope, collection, (file) => {
            for (const { id, doc } of docs) {
                file.docs[id] = cloneJson(doc);
                appendEvent(file, { id, doc: cloneJson(doc), op: 'snapshot', time: Date.now() });
            }
            file.updatedAt = Date.now();
        });
        this.notify(scope, collection);
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {unknown} payload
     */
    async ingestGetResult(scope, collection, payload) {
        const entry = docFromPayload(payload);
        if (!entry || entry.doc == null)
            return;
        await this.put(scope, collection, entry.id, entry.doc, 'snapshot');
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {string} id
     * @param {unknown} doc
     * @param {'snapshot' | 'create' | 'put' | 'patch'} op
     */
    async put(scope, collection, id, doc, op = 'put') {
        await this.enqueue(scope, collection, (file) => {
            const currentDoc = isObject(file.docs[id]) ? /** @type {Record<string, unknown>} */ (file.docs[id]) : {};
            const patchDoc = isObject(doc) ? /** @type {Record<string, unknown>} */ (doc) : {};
            const nextDoc = op === 'patch'
                ? { ...currentDoc, ...patchDoc }
                : cloneJson(doc);
            file.docs[id] = nextDoc;
            appendEvent(file, { id, doc: cloneJson(nextDoc), op, time: Date.now() });
            file.updatedAt = Date.now();
        });
        this.notify(scope, collection);
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {string} id
     */
    async delete(scope, collection, id) {
        await this.enqueue(scope, collection, (file) => {
            delete file.docs[id];
            appendEvent(file, { id, doc: null, op: 'delete', time: Date.now() });
            file.updatedAt = Date.now();
        });
        this.notify(scope, collection);
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {() => void} listener
     */
    subscribe(scope, collection, listener) {
        const key = this.collectionKey(scope, collection);
        const listeners = this.listeners.get(key) ?? new Set();
        listeners.add(listener);
        this.listeners.set(key, listeners);
        return () => {
            listeners.delete(listener);
            if (listeners.size === 0)
                this.listeners.delete(key);
        };
    }

    /**
     * @param {string} scope
     * @param {string} collection
     */
    notify(scope, collection) {
        const key = this.collectionKey(scope, collection);
        for (const listener of this.listeners.get(key) ?? []) {
            try {
                listener();
            } catch {
                // Subscriber failures should not break storage writes.
            }
        }
    }
}

/** @param {unknown} value */
function isObject(value) {
    return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

/** @returns {FyloLocalCollectionFile} */
function emptyCollection() {
    return { version: /** @type {1} */ (FILE_VERSION), docs: {}, events: [], offset: 0, updatedAt: 0 };
}

/** @param {unknown} value */
function normalizeCollectionFile(value) {
    if (!value || typeof value !== 'object')
        return emptyCollection();
    const record = /** @type {Partial<FyloLocalCollectionFile>} */ (value);
    /** @type {FyloLocalCollectionFile} */
    const file = {
        version: /** @type {1} */ (FILE_VERSION),
        docs: record.docs && typeof record.docs === 'object' && !Array.isArray(record.docs) ? cloneJson(record.docs) : {},
        events: Array.isArray(record.events) ? record.events.slice(-MAX_EVENTS).map(cloneJson) : [],
        offset: Number.isFinite(record.offset) ? Number(record.offset) : 0,
        updatedAt: Number.isFinite(record.updatedAt) ? Number(record.updatedAt) : 0,
    };
    return file;
}

/**
 * @param {FyloLocalCollectionFile} file
 * @param {FyloLocalEvent} event
 */
function appendEvent(file, event) {
    file.offset += 1;
    file.events.push(event);
    if (file.events.length > MAX_EVENTS)
        file.events.splice(0, file.events.length - MAX_EVENTS);
}

class FyloLocalWorkerClient {
    constructor() {
        /** @type {Worker | null} */
        this.worker = null;
        /** @type {FyloLocalEngine | null} */
        this.fallback = null;
        /** @type {Map<number, { resolve: (value: any) => void, reject: (error: Error) => void }>} */
        this.pending = new Map();
        /** @type {Map<string, Set<() => void>>} */
        this.listeners = new Map();
        this.nextId = 1;
        this.failed = false;
    }

    canUseWorker() {
        return typeof window !== 'undefined'
            && typeof Worker !== 'undefined'
            && !/** @type {Record<string, unknown>} */ (globalThis).__tc_prerender__;
    }

    fallbackEngine() {
        if (!this.fallback)
            this.fallback = new FyloLocalEngine();
        return this.fallback;
    }

    workerUrl() {
        if (typeof window === 'undefined')
            return './fylo-local-worker.js';
        return new URL('/fylo-local-worker.js', window.location.href).href;
    }

    ensureWorker() {
        if (!this.canUseWorker() || this.failed)
            return null;
        if (this.worker)
            return this.worker;
        try {
            this.worker = new Worker(this.workerUrl(), { type: 'module', name: 'tachyon-fylo-local' });
            this.worker.addEventListener('message', (event) => this.handleMessage(event));
            this.worker.addEventListener('error', (event) => {
                this.failed = true;
                const error = new Error(event.message || 'FYLO local worker failed');
                for (const pending of this.pending.values())
                    pending.reject(error);
                this.pending.clear();
            });
            return this.worker;
        } catch {
            this.failed = true;
            return null;
        }
    }

    /** @param {MessageEvent} event */
    handleMessage(event) {
        const message = /** @type {{ id?: number, ok?: boolean, result?: unknown, error?: string, type?: string, scope?: string, collection?: string }} */ (event.data ?? {});
        if (message.type === 'notify' && message.scope && message.collection) {
            const key = this.collectionKey(message.scope, message.collection);
            for (const listener of this.listeners.get(key) ?? []) {
                try {
                    listener();
                } catch {
                    // Listener failures should not break worker delivery.
                }
            }
            return;
        }
        if (!message.id)
            return;
        const pending = this.pending.get(message.id);
        if (!pending)
            return;
        this.pending.delete(message.id);
        if (message.ok)
            pending.resolve(message.result);
        else
            pending.reject(new Error(message.error || 'FYLO local worker request failed'));
    }

    /**
     * @param {string} type
     * @param {Record<string, unknown>} payload
     */
    async request(type, payload = {}) {
        const worker = this.ensureWorker();
        if (!worker)
            return this.fallbackRequest(type, payload);
        const id = this.nextId++;
        const promise = new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject });
        });
        worker.postMessage({ id, type, ...payload });
        return promise;
    }

    /**
     * @param {string} type
     * @param {Record<string, unknown>} payload
     */
    async fallbackRequest(type, payload) {
        const engine = this.fallbackEngine();
        switch (type) {
            case 'available': return engine.available();
            case 'find': return engine.find(String(payload.scope), String(payload.collection), /** @type {Record<string, unknown>} */ (payload.query ?? {}));
            case 'get': return engine.get(String(payload.scope), String(payload.collection), String(payload.docId));
            case 'ingestFindResult': return engine.ingestFindResult(String(payload.scope), String(payload.collection), payload.payload);
            case 'ingestGetResult': return engine.ingestGetResult(String(payload.scope), String(payload.collection), payload.payload);
            case 'put': return engine.put(String(payload.scope), String(payload.collection), String(payload.docId), payload.doc, /** @type {'snapshot' | 'create' | 'put' | 'patch'} */ (payload.op ?? 'put'));
            case 'delete': return engine.delete(String(payload.scope), String(payload.collection), String(payload.docId));
            default: throw new Error(`Unknown FYLO local worker request '${type}'`);
        }
    }

    /**
     * @param {string} scope
     * @param {string} collection
     */
    collectionKey(scope, collection) {
        return `${scope}:${collection}`;
    }

    async available() {
        return Boolean(await this.request('available'));
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {Record<string, unknown>} query
     */
    async find(scope, collection, query = {}) {
        return /** @type {FyloLocalFindResult} */ (await this.request('find', { scope, collection, query }));
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {string} id
     */
    async get(scope, collection, id) {
        return /** @type {FyloLocalGetResult | null} */ (await this.request('get', { scope, collection, docId: id }));
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {unknown} payload
     */
    async ingestFindResult(scope, collection, payload) {
        await this.request('ingestFindResult', { scope, collection, payload });
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {unknown} payload
     */
    async ingestGetResult(scope, collection, payload) {
        await this.request('ingestGetResult', { scope, collection, payload });
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {string} id
     * @param {unknown} doc
     * @param {'snapshot' | 'create' | 'put' | 'patch'} op
     */
    async put(scope, collection, id, doc, op = 'put') {
        await this.request('put', { scope, collection, docId: id, doc, op });
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {string} id
     */
    async delete(scope, collection, id) {
        await this.request('delete', { scope, collection, docId: id });
    }

    /**
     * @param {string} scope
     * @param {string} collection
     * @param {() => void} listener
     */
    subscribe(scope, collection, listener) {
        const key = this.collectionKey(scope, collection);
        const listeners = this.listeners.get(key) ?? new Set();
        listeners.add(listener);
        this.listeners.set(key, listeners);
        if (this.ensureWorker())
            void this.request('subscribe', { scope, collection });
        else {
            const unsubscribe = this.fallbackEngine().subscribe(scope, collection, listener);
            return () => {
                unsubscribe();
                listeners.delete(listener);
                if (listeners.size === 0)
                    this.listeners.delete(key);
            };
        }
        return () => {
            listeners.delete(listener);
            if (listeners.size === 0) {
                this.listeners.delete(key);
                void this.request('unsubscribe', { scope, collection });
            }
        };
    }
}

/** @returns {FyloMemoryLocalStore} */
function getMemoryStore() {
    const global = /** @type {Record<string, unknown>} */ (globalThis);
    if (global.__tc_fylo_memory_store__ instanceof FyloMemoryLocalStore)
        return global.__tc_fylo_memory_store__;
    const store = new FyloMemoryLocalStore();
    global.__tc_fylo_memory_store__ = store;
    return store;
}

/** @returns {FyloLocalCoordinator} */
export function getFyloLocalEngine() {
    const global = /** @type {Record<string, unknown>} */ (globalThis);
    if (global.__tc_fylo_local_engine__)
        return /** @type {FyloLocalCoordinator} */ (global.__tc_fylo_local_engine__);
    const engine = typeof window !== 'undefined' && !global.__tc_prerender__
        ? new FyloLocalWorkerClient()
        : new FyloLocalEngine();
    global.__tc_fylo_local_engine__ = engine;
    return engine;
}

export function resetFyloLocalEngineForTest() {
    const global = /** @type {Record<string, unknown>} */ (globalThis);
    Reflect.deleteProperty(global, '__tc_fylo_local_engine__');
    Reflect.deleteProperty(global, '__tc_fylo_memory_store__');
}
