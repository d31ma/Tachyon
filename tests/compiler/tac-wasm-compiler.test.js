// @ts-check
import { expect, test } from 'bun:test';
import { compileEchoWorker } from '../../src/compiler/wasm/tac-wasm-compiler.js';

/**
 * Re-creates exactly what the generated `tac.worker.js` runtime does when it
 * drives the wasm ABI (writeJson -> alloc, call, output_ptr/output_len -> parse).
 * @param {Uint8Array} bytes
 */
async function loadWorker(bytes) {
    const { instance } = await WebAssembly.instantiate(bytes, {});
    const exports = /** @type {Record<string, any>} */ (/** @type {unknown} */ (instance.exports));
    const encoder = new TextEncoder();
    const decoder = new TextDecoder();
    const view = () => new Uint8Array(exports.memory.buffer);

    /** @param {string} text */
    const writeText = (text) => {
        const bytesToWrite = encoder.encode(String(text));
        const ptr = exports.alloc(bytesToWrite.length || 1);
        expect(ptr).toBeGreaterThan(0);
        view().set(bytesToWrite, ptr);
        return { ptr, len: bytesToWrite.length };
    };
    /** @param {unknown} value */
    const writeJson = (value) => writeText(JSON.stringify(value ?? null));

    /** @param {string} method @param {unknown} request */
    const call = (method, request) => {
        const m = writeJson(method);
        const r = writeJson(request);
        const rawBody = request && typeof request === 'object' ? /** @type {{ body?: unknown }} */ (request).body : undefined;
        const b = writeText(typeof rawBody === 'string' ? rawBody : JSON.stringify(rawBody ?? null));
        exports.call(m.ptr, m.len, r.ptr, r.len, b.ptr, b.len);
        const ptr = exports.output_ptr();
        const len = exports.output_len();
        return JSON.parse(decoder.decode(view().subarray(ptr, ptr + len)));
    };

    return { exports, call, byteLen: (/** @type {unknown} */ v) => encoder.encode(JSON.stringify(v ?? null)).length };
}

test('in-house echo worker is worker-ABI compatible and parses as JSON', async () => {
    const bytes = compileEchoWorker({ engine: 'rust' });
    const worker = await loadWorker(bytes);

    expect(worker.exports.memory).toBeInstanceOf(WebAssembly.Memory);
    for (const name of ['alloc', 'call', 'output_ptr', 'output_len', 'dealloc'])
        expect(typeof worker.exports[name]).toBe('function');

    const request = { method: 'POST', headers: {}, body: { text: 'hello world' } };
    const response = worker.call('summarize', request);

    expect(response.status).toBe(200);
    expect(response.headers['Content-Type']).toBe('application/json');
    expect(response.body.engine).toBe('rust');
    expect(response.body.requestBytes).toBe(worker.byteLen(request));
    expect(response.body.message).toContain('no external toolchain');
});

test('echo worker handles repeated calls with varying request sizes', async () => {
    const worker = await loadWorker(compileEchoWorker());

    const small = { a: 1 };
    const large = { values: Array.from({ length: 50 }, (_unused, i) => i) };

    const first = worker.call('a', small);
    const second = worker.call('b', large);

    expect(first.body.requestBytes).toBe(worker.byteLen(small));
    expect(second.body.requestBytes).toBe(worker.byteLen(large));
    expect(second.body.requestBytes).toBeGreaterThan(first.body.requestBytes);
});

test('itoa handles a zero-length request body', async () => {
    const worker = await loadWorker(compileEchoWorker());
    // JSON.stringify(null) -> "null" (4 bytes); exercises the non-zero path.
    const response = worker.call('noop', null);
    expect(response.body.requestBytes).toBe(4);
});
