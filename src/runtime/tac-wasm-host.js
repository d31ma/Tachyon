// @ts-check
//
// In-process Tac Worker host (ABI driver).
//
// Loads an in-house-compiled Tac wasm module and invokes its worker ABI
// (`call`, `output_ptr`, `output_len`) directly on the host runtime — no Web
// Worker, no subprocess.
//
// Supports both monolithic (legacy) and split-compiled modules. Split modules
// import memory and utility functions from a shared runtime module;
// `instantiate` / `instantiateSync` auto-detect the shape and load the runtime
// automatically.
//
// The host also provides the curated request-field runtime: handlers that use
// `request.query/path/header("k")` import `env.req_query/req_path/req_header`,
// which this host implements against the request being served.

import { buildRuntimeModule } from '../compiler/wasm/tac-wasm-compiler.js';

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** @typedef {{ status: number, headers?: Record<string, string>, body?: any }} TacWorkerResponse */
/** @typedef {{ method?: string, headers?: Record<string, unknown>, paths?: Record<string, unknown>, query?: Record<string, unknown>, body?: unknown }} TacRequest */
/** @typedef {Record<string, any> & { memory: WebAssembly.Memory }} RuntimeExports */
/** @typedef {Record<string, any>} HandlerExports */

function runtimeName() {
    if (typeof Bun !== 'undefined')
        return 'bun';
    if (typeof process !== 'undefined' && process.versions?.node)
        return 'node';
    return 'javascript';
}

/** @returns {Record<string, string>} */
function platformSnapshot() {
    const nav = typeof navigator !== 'undefined' ? navigator : null;
    const proc = typeof process !== 'undefined' ? process : null;
    return {
        os: String(proc?.platform ?? nav?.platform ?? 'unknown'),
        arch: String(proc?.arch ?? 'wasm32'),
        runtime: runtimeName(),
        target: String(proc?.env?.TAC_BUNDLE_TARGETS?.split(',')[0] || 'server'),
        targets: String(proc?.env?.TAC_BUNDLE_TARGETS || 'server'),
        cpuCores: String(nav?.hardwareConcurrency ?? ''),
        language: String(nav?.language ?? ''),
        timezone: (() => {
            try { return Intl.DateTimeFormat().resolvedOptions().timeZone || ''; }
            catch { return ''; }
        })(),
        online: nav && 'onLine' in nav ? String(nav.onLine) : '',
        touch: nav && 'maxTouchPoints' in nav ? String(nav.maxTouchPoints > 0) : '',
    };
}

export class TacWasmHost {
    /** @type {WebAssembly.Module | null} */
    static _runtimeModule = null;

    static getRuntimeModule() {
        if (!TacWasmHost._runtimeModule) {
            TacWasmHost._runtimeModule = new WebAssembly.Module(buildRuntimeModule());
        }
        return TacWasmHost._runtimeModule;
    }

    /** @param {string} [label] Source label used in diagnostics. */
    constructor(label = 'tac worker') {
        this.label = label;
        /** @type {WebAssembly.Exports | null} */
        this.runtimeExports = null;
        /** @type {WebAssembly.Exports | null} */
        this.handlerExports = null;
        /** @type {TacRequest | null} The request currently being served (read by host imports). */
        this.currentRequest = null;
    }

    /**
     * Auto-detect whether a module is monolithic (exports memory) or split
     * (imports memory from env).
     * @param {WebAssembly.Module} module
     * @returns {boolean}
     */
    static isMonolithic(module) {
        const exports = WebAssembly.Module.exports(module);
        return exports.some((e) => e.name === 'memory' && e.kind === 'memory');
    }

    /**
     * Validate and store runtime + handler exports. Returns `this` for chaining.
     * For monolithic modules both arguments may be the same object.
     * @param {WebAssembly.Exports} runtimeExports
     * @param {WebAssembly.Exports} handlerExports
     * @returns {TacWasmHost}
     */
    bindExports(runtimeExports, handlerExports) {
        const isMono = runtimeExports === handlerExports;
        if (isMono) {
            if (!(runtimeExports.memory instanceof WebAssembly.Memory))
                throw new Error(`Tac worker ${this.label} must export memory`);
            for (const name of ['alloc', 'call', 'output_ptr', 'output_len']) {
                if (typeof (/** @type {Record<string, unknown>} */ (runtimeExports)[name]) !== 'function')
                    throw new Error(`Tac worker ${this.label} must export ${name}()`);
            }
        } else {
            if (!(runtimeExports.memory instanceof WebAssembly.Memory))
                throw new Error(`Tac worker ${this.label} runtime must export memory`);
            for (const name of ['alloc', 'dealloc', 'copy', 'itoa', 'mkStr', 'intToStr', 'strCat', 'jsonEscape', 'quoteStr', 'idiv', 'imod', 'setHeap']) {
                if (typeof (/** @type {Record<string, unknown>} */ (runtimeExports)[name]) !== 'function')
                    throw new Error(`Tac worker ${this.label} runtime must export ${name}()`);
            }
            if (typeof handlerExports.call !== 'function')
                throw new Error(`Tac worker ${this.label} must export call()`);
            for (const name of ['output_ptr', 'output_len']) {
                if (typeof (/** @type {Record<string, unknown>} */ (handlerExports)[name]) !== 'function')
                    throw new Error(`Tac worker ${this.label} must export ${name}()`);
            }
        }
        this.runtimeExports = runtimeExports;
        this.handlerExports = handlerExports;
        return this;
    }

    /**
     * Host functions the wasm module may import.
     * @returns {WebAssembly.Imports}
     */
    importObject() {
        return {
            env: {
                req_query: (/** @type {number} */ keyPtr, /** @type {number} */ keyLen) => this.lookupField('query', keyPtr, keyLen),
                req_path: (/** @type {number} */ keyPtr, /** @type {number} */ keyLen) => this.lookupField('paths', keyPtr, keyLen),
                req_header: (/** @type {number} */ keyPtr, /** @type {number} */ keyLen) => this.lookupField('headers', keyPtr, keyLen),
                req_platform: (/** @type {number} */ keyPtr, /** @type {number} */ keyLen) => this.lookupPlatform(keyPtr, keyLen),
            },
        };
    }

    /**
     * WASI stubs required by the runtime module.
     * @returns {Record<string, Function>}
     */
    wasiImports() {
        return {
            args_get() { return 0; },
            args_sizes_get() { return 0; },
            clock_time_get() { return 0; },
            environ_get() { return 0; },
            environ_sizes_get() { return 0; },
            fd_close() { return 0; },
            fd_fdstat_get() { return 0; },
            fd_seek() { return 0; },
            fd_write() { return 0; },
            poll_oneoff() { return 0; },
            proc_exit: (/** @type {number} */ code) => {
                throw new Error(`Tac worker ${this.label} exited with code ${code}`);
            },
            random_get() { return 0; },
        };
    }

    /**
     * Resolve a request field by key and hand the value back to the wasm as a
     * string value (empty string when absent).
     * @param {'query' | 'paths' | 'headers'} kind
     * @param {number} keyPtr
     * @param {number} keyLen
     * @returns {number} String-header pointer.
     */
    lookupField(kind, keyPtr, keyLen) {
        const key = decoder.decode(this.view().subarray(keyPtr, keyPtr + keyLen));
        const source = this.currentRequest && typeof this.currentRequest === 'object'
            ? /** @type {Record<string, unknown> | undefined} */ (this.currentRequest[kind])
            : undefined;
        const value = source && typeof source === 'object' ? source[key] : undefined;
        const text = value == null ? '' : (typeof value === 'string' ? value : String(value));
        return this.writeStringValue(text);
    }

    /**
     * Read a curated host/platform fact. This is the only OS-like surface the
     * in-house worker ABI exposes, keeping pages/components out of the platform
     * boundary and avoiding arbitrary system calls.
     * @param {number} keyPtr
     * @param {number} keyLen
     * @returns {number}
     */
    lookupPlatform(keyPtr, keyLen) {
        const key = decoder.decode(this.view().subarray(keyPtr, keyPtr + keyLen));
        const value = platformSnapshot()[key] ?? '';
        return this.writeStringValue(value);
    }

    /**
     * Allocate a string value: a `{ dataPtr@0, byteLen@4 }` header pointing at
     * freshly-written UTF-8 bytes. Returns the header pointer.
     * @param {string} text
     * @returns {number}
     */
    writeStringValue(text) {
        const runtime = this.requireRuntimeExports();
        const bytes = encoder.encode(text);
        const dataPtr = runtime.alloc(bytes.length || 1);
        const header = runtime.alloc(8);
        // Fetch the view after both allocs (alloc may have grown/detached memory).
        new Uint8Array(runtime.memory.buffer).set(bytes, dataPtr);
        const dv = new DataView(runtime.memory.buffer);
        dv.setInt32(header, dataPtr, true);
        dv.setInt32(header + 4, bytes.length, true);
        return header;
    }

    /**
     * Instantiate a Tac worker module from its wasm bytes.
     * Auto-detects monolithic vs split and loads the runtime automatically.
     * @param {BufferSource} wasmBytes
     * @param {string} [label]
     * @returns {Promise<TacWasmHost>}
     */
    static async instantiate(wasmBytes, label) {
        const host = new TacWasmHost(label);
        const handlerModule = await WebAssembly.compile(wasmBytes);
        if (TacWasmHost.isMonolithic(handlerModule)) {
            const { instance } = await WebAssembly.instantiate(handlerModule, host.importObject());
            return host.bindExports(instance.exports, instance.exports);
        }
        const runtimeModule = TacWasmHost.getRuntimeModule();
        const runtimeInstance = new WebAssembly.Instance(runtimeModule, { wasi_snapshot_preview1: host.wasiImports() });
        const handlerInstance = new WebAssembly.Instance(handlerModule, {
            env: host._makeHandlerEnv(runtimeInstance.exports),
        });
        return host.bindExports(runtimeInstance.exports, handlerInstance.exports);
    }

    /**
     * Synchronously instantiate a Tac worker module.
     * Auto-detects monolithic vs split and loads the runtime automatically.
     * @param {BufferSource} wasmBytes
     * @param {string} [label]
     * @returns {TacWasmHost}
     */
    static instantiateSync(wasmBytes, label) {
        const host = new TacWasmHost(label);
        const handlerModule = new WebAssembly.Module(wasmBytes);
        if (TacWasmHost.isMonolithic(handlerModule)) {
            const instance = new WebAssembly.Instance(handlerModule, host.importObject());
            return host.bindExports(instance.exports, instance.exports);
        }
        const runtimeModule = TacWasmHost.getRuntimeModule();
        const runtimeInstance = new WebAssembly.Instance(runtimeModule, { wasi_snapshot_preview1: host.wasiImports() });
        const handlerInstance = new WebAssembly.Instance(handlerModule, {
            env: host._makeHandlerEnv(runtimeInstance.exports),
        });
        return host.bindExports(runtimeInstance.exports, handlerInstance.exports);
    }

    /**
     * Build the `env` import object for a split handler module.
     * @param {WebAssembly.Exports} runtimeExports
     * @returns {WebAssembly.ModuleImports}
     */
    _makeHandlerEnv(runtimeExports) {
        return /** @type {WebAssembly.ModuleImports} */ ({
            memory: runtimeExports.memory,
            alloc: runtimeExports.alloc,
            dealloc: runtimeExports.dealloc,
            copy: runtimeExports.copy,
            itoa: runtimeExports.itoa,
            mkStr: runtimeExports.mkStr,
            intToStr: runtimeExports.intToStr,
            strCat: runtimeExports.strCat,
            jsonEscape: runtimeExports.jsonEscape,
            quoteStr: runtimeExports.quoteStr,
            idiv: runtimeExports.idiv,
            imod: runtimeExports.imod,
            setHeap: runtimeExports.setHeap,
            ...this.importObject().env,
        });
    }

    /** @returns {RuntimeExports} */
    requireRuntimeExports() {
        if (!this.runtimeExports)
            throw new Error(`Tac worker ${this.label} runtime is not bound`);
        return /** @type {RuntimeExports} */ (this.runtimeExports);
    }

    /** @returns {HandlerExports} */
    requireHandlerExports() {
        if (!this.handlerExports)
            throw new Error(`Tac worker ${this.label} handler is not bound`);
        return /** @type {HandlerExports} */ (this.handlerExports);
    }

    /**
     * Fresh byte view of linear memory.
     * @returns {Uint8Array}
     */
    view() {
        return new Uint8Array(this.requireRuntimeExports().memory.buffer);
    }

    /**
     * Copy UTF-8 text into freshly-allocated linear memory.
     * @param {string} text
     * @returns {{ ptr: number, len: number }}
     */
    writeText(text) {
        const runtime = this.requireRuntimeExports();
        const bytes = encoder.encode(String(text));
        const ptr = runtime.alloc(bytes.length || 1);
        if (!ptr)
            throw new Error(`Tac worker ${this.label} failed to allocate ${bytes.length} bytes`);
        this.view().set(bytes, ptr);
        return { ptr, len: bytes.length };
    }

    /** @param {unknown} value @returns {{ ptr: number, len: number }} */
    writeJson(value) {
        return this.writeText(JSON.stringify(value ?? null));
    }

    /** @param {{ ptr: number, len: number } | null} input */
    dealloc(input) {
        const runtime = this.requireRuntimeExports();
        if (input && typeof runtime.dealloc === 'function')
            runtime.dealloc(input.ptr, input.len);
    }

    /**
     * Read and JSON-parse the worker's output buffer.
     * @returns {any}
     */
    readOutput() {
        const handler = this.requireHandlerExports();
        const ptr = handler.output_ptr();
        const len = handler.output_len();
        if (!len)
            return {};
        const text = decoder.decode(this.view().subarray(ptr, ptr + len));
        try {
            return JSON.parse(text);
        }
        catch {
            throw new Error(`Tac worker ${this.label} produced invalid JSON output: ${text.slice(0, 200)}`);
        }
    }

    /**
     * Invoke a handler method. `request` is the full request envelope the
     * handler sees via `request.json()`; its `.body` becomes `request.body()`
     * and its `query`/`paths`/`headers` back the `request.query/path/header`
     * accessors.
     * @param {string} method HTTP verb, e.g. 'GET'.
     * @param {TacRequest} request
     * @returns {TacWorkerResponse}
     */
    call(method, request) {
        this.currentRequest = request;
        const rawBody = request && typeof request === 'object' ? request.body : undefined;
        const bodyText = typeof rawBody === 'string' ? rawBody : JSON.stringify(rawBody ?? null);
        const methodInput = this.writeJson(method);
        const requestInput = this.writeJson(request);
        const bodyInput = this.writeText(bodyText);
        try {
            this.requireHandlerExports().call(
                methodInput.ptr, methodInput.len,
                requestInput.ptr, requestInput.len,
                bodyInput.ptr, bodyInput.len,
            );
            return this.readOutput();
        }
        finally {
            this.dealloc(methodInput);
            this.dealloc(requestInput);
            this.dealloc(bodyInput);
            this.currentRequest = null;
        }
    }
}
