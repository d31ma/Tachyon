// @ts-check

/**
 * @typedef {RequestInit & { body?: BodyInit | Record<string, unknown> | unknown[] | null, tac?: { poolSize?: number, workers?: number } }} TacWorkerRequestInit
 * @typedef {{ status?: number, statusText?: string, headers?: Record<string, string> | [string, string][], body?: unknown }} TacWorkerResponsePayload
 */

const MAX_TAC_WORKER_POOL_SIZE = 16;

/** @param {string | URL} specifier */
function isTacWorkerSpecifier(specifier) {
    return typeof specifier === 'string' && specifier.startsWith('tac://');
}

/** @param {string} specifier */
function resolveTacWorkerUrl(specifier) {
    return `/workers/${resolveTacWorkerRoute(specifier)}/tac.worker.js`;
}

/** @param {string} specifier */
function resolveTacWorkerRoute(specifier) {
    const parsed = new URL(specifier);
    const route = [parsed.hostname, parsed.pathname]
        .join('/')
        .replace(/\/+/g, '/')
        .replace(/^\/|\/$/g, '');
    if (!route)
        throw new Error(`Tac worker specifier '${specifier}' must include a route, e.g. tac://language/rust`);
    if (!/^[a-z][a-z0-9-]*(?:\/[a-z][a-z0-9-]*)*$/.test(route))
        throw new Error(`Tac worker specifier '${specifier}' must use lowercase alphanumeric or hyphenated route segments`);
    return route;
}

/** @param {unknown} value */
function parsePoolSize(value) {
    if (value === undefined || value === null || value === '')
        return null;
    const parsed = Number(value);
    if (!Number.isFinite(parsed))
        return null;
    return Math.max(1, Math.min(MAX_TAC_WORKER_POOL_SIZE, Math.floor(parsed)));
}

/**
 * @param {string} specifier
 * @param {TacWorkerRequestInit} init
 */
function resolvePoolSize(specifier, init) {
    const optionSize = parsePoolSize(init.tac?.poolSize ?? init.tac?.workers);
    if (optionSize)
        return optionSize;
    const headerSize = init.headers ? parsePoolSize(new Headers(init.headers).get('x-tac-workers')) : null;
    if (headerSize)
        return headerSize;
    const parsed = new URL(specifier);
    return parsePoolSize(parsed.searchParams.get('pool') ?? parsed.searchParams.get('workers')) ?? 1;
}

/** @param {unknown} body */
async function serializeBody(body) {
    if (body === undefined || body === null)
        return null;
    if (typeof body === 'string')
        return body;
    if (body instanceof URLSearchParams)
        return body.toString();
    if (body instanceof Blob)
        return body.text();
    if (body instanceof ArrayBuffer)
        return Array.from(new Uint8Array(body));
    if (ArrayBuffer.isView(body)) {
        const view = /** @type {ArrayBufferView} */ (body);
        return Array.from(new Uint8Array(view.buffer, view.byteOffset, view.byteLength));
    }
    return body;
}

/** @param {HeadersInit | undefined} headers */
function serializeHeaders(headers) {
    if (!headers)
        return {};
    return Object.fromEntries(new Headers(headers).entries());
}

/** @param {TacWorkerResponsePayload} payload */
function createResponse(payload) {
    const headers = new Headers(payload.headers ?? {});
    let body = payload.body ?? null;
    if (body !== null && typeof body !== 'string' && !(body instanceof Blob) && !(body instanceof ArrayBuffer)) {
        if (!headers.has('Content-Type'))
            headers.set('Content-Type', 'application/json');
        body = JSON.stringify(body);
    }
    return new Response(/** @type {BodyInit | null} */ (body), {
        status: payload.status ?? 200,
        statusText: payload.statusText,
        headers,
    });
}

class TacWorkerClient {
    /** @type {globalThis.Worker} */
    #worker;
    /** @type {Map<number, { resolve: (response: Response) => void, reject: (error: Error) => void }>} */
    #pending = new Map();
    #sequence = 0;

    /**
     * @param {string} specifier
     * @param {WorkerOptions} [options]
     */
    constructor(specifier, options = {}) {
        const NativeWorker = globalThis.Worker;
        if (typeof NativeWorker !== 'function') {
            throw new Error('Tac workers require the browser Worker API');
        }
        this.#worker = new NativeWorker(resolveTacWorkerUrl(specifier), {
            ...options,
            type: 'module',
        });
        this.#worker.addEventListener('message', (event) => {
            const message = event.data;
            if (!message || typeof message !== 'object')
                return;
            const id = Number(message.id);
            const pending = this.#pending.get(id);
            if (!pending)
                return;
            this.#pending.delete(id);
            if (message.ok) {
                pending.resolve(createResponse(message.response ?? {}));
            }
            else {
                pending.reject(new Error(String(message.error || 'Tac worker call failed')));
            }
        });
        this.#worker.addEventListener('error', (event) => {
            const error = new Error(event.message || 'Tac worker failed');
            for (const pending of this.#pending.values())
                pending.reject(error);
            this.#pending.clear();
        });
    }

    /**
     * @param {string} method
     * @param {TacWorkerRequestInit} [init]
     * @returns {Promise<Response>}
     */
    async fetch(method, init = {}) {
        if (!method || typeof method !== 'string')
            throw new TypeError('Tac worker fetch(method, init) requires a method name');
        const id = ++this.#sequence;
        const request = {
            method: init.method ?? 'POST',
            headers: serializeHeaders(init.headers),
            body: await serializeBody(init.body),
        };
        return new Promise((resolve, reject) => {
            this.#pending.set(id, { resolve, reject });
            this.#worker.postMessage({ id, method, request });
        });
    }

    get pendingCount() {
        return this.#pending.size;
    }

    terminate() {
        this.#worker.terminate();
        for (const pending of this.#pending.values())
            pending.reject(new Error('Tac worker was terminated'));
        this.#pending.clear();
    }

    /**
     * @param {string} type
     * @param {EventListenerOrEventListenerObject} listener
     * @param {boolean | AddEventListenerOptions} [options]
     */
    addEventListener(type, listener, options) {
        this.#worker.addEventListener(type, listener, options);
    }

    /**
     * @param {string} type
     * @param {EventListenerOrEventListenerObject} listener
     * @param {boolean | EventListenerOptions} [options]
     */
    removeEventListener(type, listener, options) {
        this.#worker.removeEventListener(type, listener, options);
    }
}

class TacWorkerPool {
    /** @type {string} */
    #specifier;
    /** @type {TacWorkerClient[]} */
    #clients = [];
    #cursor = 0;

    /**
     * @param {string} specifier
     * @param {number} poolSize
     */
    constructor(specifier, poolSize) {
        this.#specifier = specifier;
        this.ensureSize(poolSize);
    }

    /** @param {number} poolSize */
    ensureSize(poolSize) {
        const nextSize = Math.max(1, Math.min(MAX_TAC_WORKER_POOL_SIZE, poolSize));
        while (this.#clients.length < nextSize) {
            this.#clients.push(new TacWorkerClient(this.#specifier));
        }
    }

    /**
     * @param {string} method
     * @param {TacWorkerRequestInit} init
     */
    fetch(method, init = {}) {
        const minPending = Math.min(...this.#clients.map((client) => client.pendingCount));
        const candidates = this.#clients.filter((client) => client.pendingCount === minPending);
        const client = candidates[this.#cursor % candidates.length];
        this.#cursor += 1;
        return client.fetch(method, init);
    }

    terminate() {
        for (const client of this.#clients)
            client.terminate();
        this.#clients = [];
    }
}

/**
 * One cached worker pool per `tac://` route, so repeated fetches reuse browser
 * Workers and can opt into parallelism for heavier compute.
 * @type {Map<string, TacWorkerPool>}
 */
const tacWorkerPools = new Map();

/** @param {RequestInfo | URL} input */
function specifierOf(input) {
    if (typeof input === 'string')
        return input;
    if (input instanceof URL)
        return input.href;
    if (input && typeof input === 'object' && 'url' in input)
        return String(/** @type {{ url: unknown }} */ (input).url);
    return '';
}

/**
 * Tac-aware `fetch`. A `tac://<route>` URL is dispatched to the matching Tac
 * Worker - the `requestInit.method` (default `GET`) selects the handler method,
 * exactly like a Yon route - and the worker's response is returned as a real
 * `Response`. Every other URL is delegated to the platform `fetch`, so
 * local-first caching and ordinary requests are unaffected.
 *
 * @param {RequestInfo | URL} input
 * @param {TacWorkerRequestInit} [init]
 * @returns {Promise<Response>}
 */
export async function fetch(input, init = {}) {
    const specifier = specifierOf(input);
    if (!isTacWorkerSpecifier(specifier))
        return globalThis.fetch(/** @type {RequestInfo | URL} */ (input), init);
    const route = resolveTacWorkerRoute(specifier);
    const poolSize = resolvePoolSize(specifier, init);
    let pool = tacWorkerPools.get(route);
    if (!pool) {
        pool = new TacWorkerPool(specifier, poolSize);
        tacWorkerPools.set(route, pool);
    }
    else {
        pool.ensureSize(poolSize);
    }
    const method = String(init.method ?? 'GET').toUpperCase();
    return pool.fetch(method, { ...init, method });
}

export function resetTacWorkerPoolsForTest() {
    for (const pool of tacWorkerPools.values())
        pool.terminate();
    tacWorkerPools.clear();
}
