// @ts-check

/**
 * @typedef {RequestInit & { body?: BodyInit | Record<string, unknown> | unknown[] | null, tac?: { poolSize?: number, workers?: number } }} TacWorkerRequestInit
 * @typedef {{ status?: number, statusText?: string, headers?: Record<string, string> | [string, string][], body?: unknown }} TacWorkerResponsePayload
 * @typedef {{ capability: string, payload?: unknown, source?: string }} TacNativeCapabilityRequest
 * @typedef {{ ok: true, value: unknown } | { ok: false, error: string }} TacNativeCapabilityResult
 */

const MAX_TAC_WORKER_POOL_SIZE = 16;

/** @param {string | URL} specifier */
function isTacWorkerSpecifier(specifier) {
    return typeof specifier === 'string' && specifier.startsWith('tac://');
}

/** @param {string} specifier */
function resolveTacWorkerUrl(specifier) {
    const route = resolveTacWorkerRoute(specifier);
    if (typeof location !== 'undefined' && location.protocol === 'file:' && typeof document !== 'undefined') {
        return new URL(`workers/${route}/tac.worker.js`, document.baseURI).href;
    }
    return `/workers/${route}/tac.worker.js`;
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

/** @param {unknown} value */
function isRecord(value) {
    return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function appInfo() {
    const nav = typeof navigator !== 'undefined' ? navigator : null;
    const location = typeof window !== 'undefined' ? window.location : null;
    return {
        name: typeof document !== 'undefined' ? document.title || 'Tachyon App' : 'Tachyon App',
        runtime: 'tac-native-broker',
        href: location?.href ?? '',
        userAgent: nav?.userAgent ?? '',
        language: nav?.language ?? '',
        online: nav && 'onLine' in nav ? Boolean(nav.onLine) : null,
    };
}

function hasServerSideOsApis() {
    return typeof Bun !== 'undefined' || (typeof process !== 'undefined' && !!process.versions?.node);
}

/** @param {unknown} value */
function requireString(value) {
    if (typeof value !== 'string' || value.length === 0)
        throw new Error('Raw OS capability payload requires a non-empty string value');
    return value;
}

/**
 * Browser-safe default capability implementations. Native hosts can override
 * these by exposing `window.__tcNativeBridge__.invoke`.
 * @param {TacNativeCapabilityRequest} request
 * @returns {Promise<unknown>}
 */
async function dispatchBuiltInNativeCapability(request) {
    const payload = /** @type {Record<string, unknown>} */ (isRecord(request.payload) ? request.payload : {});
    const globals = /** @type {Record<string, any>} */ (globalThis);
    switch (request.capability) {
        case 'app.info':
            return appInfo();
        case 'clipboard.readText': {
            const clipboard = typeof navigator !== 'undefined' ? navigator.clipboard : null;
            if (!clipboard?.readText)
                throw new Error('clipboard.readText is not available in this environment');
            return await clipboard.readText();
        }
        case 'clipboard.writeText': {
            const clipboard = typeof navigator !== 'undefined' ? navigator.clipboard : null;
            if (!clipboard?.writeText)
                throw new Error('clipboard.writeText is not available in this environment');
            await clipboard.writeText(String(payload.text ?? ''));
            return { written: true };
        }
        case 'openUrl': {
            const url = String(payload.url ?? '');
            if (!/^https?:\/\//i.test(url))
                throw new Error('openUrl requires an http(s) URL');
            if (typeof window === 'undefined' || typeof window.open !== 'function')
                throw new Error('openUrl is not available in this environment');
            window.open(url, '_blank', 'noopener,noreferrer');
            return { opened: true };
        }
        case 'file.openText': {
            const picker = typeof globals.showOpenFilePicker === 'function'
                ? globals.showOpenFilePicker
                : null;
            if (!picker)
                throw new Error('file.openText requires the File System Access API or a native host implementation');
            const [handle] = await picker({
                multiple: false,
                types: [{ description: 'Text files', accept: { 'text/*': ['.txt', '.md', '.json', '.csv'] } }],
            });
            const file = await handle.getFile();
            return { name: file.name, text: await file.text() };
        }
        case 'fs.readText': {
            if (!hasServerSideOsApis())
                throw new Error('fs.readText requires a native host OS implementation');
            const { readFile } = await import('node:fs/promises');
            const filePath = requireString(payload.path);
            return { path: filePath, text: await readFile(filePath, 'utf8') };
        }
        case 'fs.writeText': {
            if (!hasServerSideOsApis())
                throw new Error('fs.writeText requires a native host OS implementation');
            const { writeFile } = await import('node:fs/promises');
            const filePath = requireString(payload.path);
            const text = String(payload.text ?? '');
            await writeFile(filePath, text, 'utf8');
            return { path: filePath, bytes: new TextEncoder().encode(text).byteLength, written: true };
        }
        case 'fs.readDir': {
            if (!hasServerSideOsApis())
                throw new Error('fs.readDir requires a native host OS implementation');
            const { readdir } = await import('node:fs/promises');
            const dirPath = requireString(payload.path);
            const entries = await readdir(dirPath, { withFileTypes: true });
            return {
                path: dirPath,
                entries: entries.map((entry) => ({ name: entry.name, type: entry.isDirectory() ? 'directory' : 'file' })),
            };
        }
        case 'shell.exec': {
            if (!hasServerSideOsApis())
                throw new Error('shell.exec requires a native host OS implementation');
            const command = requireString(payload.command);
            const args = Array.isArray(payload.args) ? payload.args.map((arg) => String(arg)) : [];
            const cwd = typeof payload.cwd === 'string' && payload.cwd ? payload.cwd : undefined;
            if (typeof Bun !== 'undefined') {
                const proc = Bun.spawn([command, ...args], {
                    cwd,
                    stdout: 'pipe',
                    stderr: 'pipe',
                });
                const [stdout, stderr, exitCode] = await Promise.all([
                    new Response(proc.stdout).text(),
                    new Response(proc.stderr).text(),
                    proc.exited,
                ]);
                return { command, args, cwd: cwd ?? '', exitCode, stdout, stderr };
            }
            const { spawn } = await import('node:child_process');
            return await new Promise((resolve, reject) => {
                const child = spawn(command, args, { cwd, shell: false });
                let stdout = '';
                let stderr = '';
                child.stdout.setEncoding('utf8');
                child.stderr.setEncoding('utf8');
                child.stdout.on('data', (chunk) => { stdout += chunk; });
                child.stderr.on('data', (chunk) => { stderr += chunk; });
                child.on('error', reject);
                child.on('close', (exitCode) => resolve({ command, args, cwd: cwd ?? '', exitCode: exitCode ?? -1, stdout, stderr }));
            });
        }
        default:
            throw new Error(`Native capability '${request.capability}' is not implemented`);
    }
}

/**
 * Dispatches a declared Tac native capability from a worker request. App tests
 * or host shells may install `globalThis.__tcNativeCapabilities__`; native
 * WebView hosts expose `window.__tcNativeBridge__.invoke`; otherwise a small
 * browser-safe fallback set is used.
 * @param {TacNativeCapabilityRequest} request
 * @returns {Promise<TacNativeCapabilityResult>}
 */
async function dispatchNativeCapability(request) {
    try {
        if (!request.capability || typeof request.capability !== 'string')
            throw new Error('Native capability requests require a string capability name');
        const globals = /** @type {Record<string, any>} */ (globalThis);
        const overrides = globals.__tcNativeCapabilities__;
        if (overrides && typeof overrides[request.capability] === 'function') {
            return { ok: true, value: await overrides[request.capability](request.payload, request) };
        }
        const bridge = typeof window !== 'undefined'
            ? /** @type {{ invoke?: Function } | undefined} */ (/** @type {any} */ (window).__tcNativeBridge__)
            : undefined;
        if (bridge && typeof bridge.invoke === 'function') {
            return { ok: true, value: await bridge.invoke(request.capability, request.payload ?? {}, { source: request.source ?? '' }) };
        }
        return { ok: true, value: await dispatchBuiltInNativeCapability(request) };
    }
    catch (error) {
        return { ok: false, error: error instanceof Error ? error.message : String(error) };
    }
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
            if (message.type === 'tac:native-request') {
                void dispatchNativeCapability({
                    capability: String(message.capability ?? ''),
                    payload: message.payload,
                    source: typeof message.source === 'string' ? message.source : '',
                }).then((result) => {
                    this.#worker.postMessage({
                        type: 'tac:native-response',
                        nativeId: message.nativeId,
                        ...result,
                    });
                });
                return;
            }
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

/** @returns {Set<string>} */
function nativeWorkerRoutes() {
    if (typeof document === 'undefined')
        return new Set();
    const value = document.querySelector('meta[name="tachyon-native-workers"]')?.getAttribute('content') ?? '';
    return new Set(value.split(',').map((route) => route.trim()).filter(Boolean));
}

/** @returns {string[]} */
function authorizedNativeCapabilities() {
    if (typeof document === 'undefined')
        return [];
    const value = document.querySelector('meta[name="tachyon-native-capabilities"]')?.getAttribute('content') ?? '';
    return value.split(',').map((capability) => capability.trim()).filter(Boolean);
}

/** @param {string} capability */
function isNativeCapabilityAuthorized(capability) {
    return authorizedNativeCapabilities().some((entry) => {
        if (entry === capability)
            return true;
        return entry.endsWith('.*') && capability.startsWith(entry.slice(0, -1));
    });
}

/** @param {unknown} response */
function nativeCapabilityFromWorkerResponse(response) {
    if (!isRecord(response))
        return null;
    const responseRecord = /** @type {Record<string, unknown>} */ (response);
    const body = isRecord(responseRecord.body)
        ? /** @type {Record<string, unknown>} */ (responseRecord.body)
        : null;
    const result = body && isRecord(body.result)
        ? /** @type {Record<string, unknown>} */ (body.result)
        : null;
    const candidate = body && isRecord(body.$tacNative)
        ? body.$tacNative
        : result && isRecord(result.$tacNative)
            ? result.$tacNative
            : isRecord(responseRecord.$tacNative)
                ? responseRecord.$tacNative
                : null;
    if (!candidate)
        return null;
    const capabilityRequest = /** @type {Record<string, unknown>} */ (candidate);
    return {
        capability: String(capabilityRequest.capability ?? ''),
        payload: capabilityRequest.payload ?? {},
        status: Number(capabilityRequest.status ?? responseRecord.status ?? 200),
        headers: isRecord(capabilityRequest.headers) ? capabilityRequest.headers : {},
    };
}

/** @param {string} route */
function isNativeWorkerHost(route) {
    return nativeWorkerRoutes().has(route) && !!ensureNativeBridge();
}

/**
 * Native WebView script injection can run in a content world that is not
 * visible to page modules. If the low-level host object exists, synthesize the
 * page-world bridge lazily so `tac://` native workers still have one stable
 * browser-facing contract.
 * @returns {{ invoke: Function, postMessage?: Function, messageHandler?: Function } | undefined}
 */
function ensureNativeBridge() {
    if (typeof window === 'undefined')
        return undefined;
    const windowRecord = /** @type {Record<string, any>} */ (window);
    const existing = windowRecord.__tcNativeBridge__;
    if (existing && typeof existing.invoke === 'function')
        return existing;
    const nativeHost = windowRecord.webkit?.messageHandlers?.tachyon ?? windowRecord.__tcNativeHost__;
    if (!nativeHost || typeof nativeHost.postMessage !== 'function')
        return undefined;

    let sequence = 0;
    /** @type {Map<number, { resolve: (value: unknown) => void, reject: (error: Error) => void, timeout: ReturnType<typeof setTimeout> }>} */
    const pending = new Map();
    /** @type {Set<(message: unknown) => void>} */
    const listeners = new Set();

    /** @param {unknown} message */
    function postMessage(message) {
        nativeHost.postMessage(typeof message === 'string' ? message : JSON.stringify(message));
        return true;
    }

    /** @param {unknown} raw */
    function messageHandler(raw) {
        let message = raw;
        if (typeof raw === 'string') {
            try {
                message = JSON.parse(raw);
            }
            catch {
                message = { type: 'message', value: raw };
            }
        }
        if (message && typeof message === 'object' && /** @type {{ type?: unknown }} */ (message).type === 'tac:native-response') {
            const response = /** @type {{ id?: unknown, ok?: unknown, value?: unknown, error?: unknown }} */ (message);
            const id = Number(response.id);
            const pendingCall = pending.get(id);
            if (!pendingCall)
                return;
            pending.delete(id);
            clearTimeout(pendingCall.timeout);
            if (response.ok)
                pendingCall.resolve(response.value);
            else
                pendingCall.reject(new Error(String(response.error || 'Native capability failed')));
            return;
        }
        for (const listener of listeners)
            listener(message);
    }

    /**
     * @param {string} capability
     * @param {unknown} [payload]
     * @param {{ timeoutMs?: number, source?: string }} [options]
     */
    function invoke(capability, payload = {}, options = {}) {
        const id = ++sequence;
        return new Promise((resolve, reject) => {
            const timeout = setTimeout(() => {
                pending.delete(id);
                reject(new Error(`Native capability '${capability}' timed out`));
            }, Number(options.timeoutMs || 10000));
            pending.set(id, { resolve, reject, timeout });
            try {
                postMessage({
                    type: 'tac:native-request',
                    id,
                    capability,
                    payload,
                    source: String(options.source || ''),
                });
            }
            catch (error) {
                clearTimeout(timeout);
                pending.delete(id);
                reject(error instanceof Error ? error : new Error(String(error)));
            }
        });
    }

    windowRecord.__tcNativeBridge__ = {
        version: 2,
        postMessage,
        invoke,
        /** @param {(message: unknown) => void} handler */
        onMessage(handler) {
            listeners.add(handler);
            return () => listeners.delete(handler);
        },
        messageHandler,
    };
    return windowRecord.__tcNativeBridge__;
}

class TacNativeWorkerClient {
    /** @type {string} */
    #route;
    /** @type {Map<number, { resolve: (response: Response) => void, reject: (error: Error) => void }>} */
    #pending = new Map();
    #sequence = 0;

    /**
     * @param {string} specifier
     */
    constructor(specifier) {
        const bridge = ensureNativeBridge();
        if (!bridge || typeof bridge.invoke !== 'function') {
            throw new Error('Tac native workers require window.__tcNativeBridge__.invoke');
        }
        this.#route = resolveTacWorkerRoute(specifier);
    }

    /**
     * @param {string} method
     * @param {TacWorkerRequestInit} [init]
     * @returns {Promise<Response>}
     */
    async fetch(method, init = {}) {
        if (!method || typeof method !== 'string')
            throw new TypeError('Tac worker fetch(method, init) requires a method name');
        const bridge = /** @type {{ invoke: Function }} */ (ensureNativeBridge());
        const request = {
            method: init.method ?? 'POST',
            headers: serializeHeaders(init.headers),
            body: await serializeBody(init.body),
        };
        const response = await bridge.invoke('tachyon.worker', {
            route: this.#route,
            method,
            request,
        });
        const nativeRequest = nativeCapabilityFromWorkerResponse(response);
        if (!nativeRequest)
            return createResponse(response && typeof response === 'object' ? response : {});
        if (!nativeRequest.capability || !isNativeCapabilityAuthorized(nativeRequest.capability)) {
            throw new Error(
                `Tac native worker capability '${nativeRequest.capability}' is not authorized; ` +
                'add it to TAC_NATIVE_CAPABILITIES',
            );
        }
        const result = await dispatchNativeCapability({
            capability: nativeRequest.capability,
            payload: nativeRequest.payload,
            source: this.#route,
        });
        if (!result.ok)
            throw new Error(result.error);
        return createResponse({
            status: nativeRequest.status,
            headers: /** @type {Record<string, string>} */ (nativeRequest.headers),
            body: result.value,
        });
    }

    get pendingCount() {
        return 0;
    }

    terminate() {
        // Native workers are stateless per invocation; nothing to terminate.
    }

    /**
     * @param {string} _type
     * @param {EventListenerOrEventListenerObject} _listener
     * @param {boolean | AddEventListenerOptions} [_options]
     */
    addEventListener(_type, _listener, _options) {
        // No-op for native workers.
    }

    /**
     * @param {string} _type
     * @param {EventListenerOrEventListenerObject} _listener
     * @param {boolean | EventListenerOptions} [_options]
     */
    removeEventListener(_type, _listener, _options) {
        // No-op for native workers.
    }
}

class TacWorkerPool {
    /** @type {string} */
    #specifier;
    /** @type {(TacWorkerClient | TacNativeWorkerClient)[]} */
    #clients = [];
    #cursor = 0;
    #useNative = false;

    /**
     * @param {string} specifier
     * @param {number} poolSize
     */
    constructor(specifier, poolSize) {
        this.#specifier = specifier;
        this.#useNative = isNativeWorkerHost(resolveTacWorkerRoute(specifier));
        this.ensureSize(poolSize);
    }

    /** @param {number} poolSize */
    ensureSize(poolSize) {
        const nextSize = Math.max(1, Math.min(MAX_TAC_WORKER_POOL_SIZE, poolSize));
        const Client = this.#useNative ? TacNativeWorkerClient : TacWorkerClient;
        while (this.#clients.length < nextSize) {
            this.#clients.push(new Client(this.#specifier));
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
