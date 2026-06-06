// @ts-check
import { expect, test } from 'bun:test';
import { compilePythonWorker, significantLines, Parser } from '../../src/compiler/wasm/python-compiler.js';

/** @param {Uint8Array} bytes */
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
        view().set(bytesToWrite, ptr);
        return { ptr, len: bytesToWrite.length };
    };
    /** @param {unknown} value */
    const writeJson = (value) => writeText(JSON.stringify(value ?? null));
    /** @param {string} verb @param {unknown} request */
    const call = (verb, request) => {
        const m = writeJson(verb);
        const r = writeJson(request);
        const rawBody = request && typeof request === 'object' ? /** @type {{ body?: unknown }} */ (request).body : undefined;
        const b = writeText(typeof rawBody === 'string' ? rawBody : JSON.stringify(rawBody ?? null));
        exports.call(m.ptr, m.len, r.ptr, r.len, b.ptr, b.len);
        return JSON.parse(decoder.decode(view().subarray(exports.output_ptr(), exports.output_ptr() + exports.output_len())));
    };
    return { call, byteLen: (/** @type {unknown} */ v) => encoder.encode(JSON.stringify(v ?? null)).length };
}

const SOURCE = `
from example import ignored

class Handler:
    @staticmethod
    def GET(request) -> int:
        return request.len() * 2

    @staticmethod
    def POST(request) -> str:
        size = request.len()
        tier = "big" if size > 32 else "small"
        return "size: " + tier

    @staticmethod
    def PUT(request) -> str:
        return "echo: " + request.body()

    @staticmethod
    def PATCH(request) -> int:
        total = 0
        index = 0
        while index < 5:
            total = total + index
            index = index + 1
        return total
`;

test('parser builds a verb-named method list from the Python class shape', () => {
    const program = new Parser(significantLines(SOURCE)).parseProgram();
    expect(program.methods.map((m) => m.name)).toEqual(['GET', 'POST', 'PUT', 'PATCH']);
    expect(program.methods.map((m) => m.returnType)).toEqual(['i32', 'str', 'str', 'i32']);
});

test('dispatches by verb; compiles ternary, while, concat, and request.body()', async () => {
    const worker = await loadWorker(compilePythonWorker(SOURCE));

    const request = { method: 'POST', body: 'hi' };
    expect(worker.call('GET', request).body.result).toBe(worker.byteLen(request) * 2);

    expect(worker.call('POST', { hello: 'world wide web' }).body.result)
        .toBe(worker.byteLen({ hello: 'world wide web' }) > 32 ? 'size: big' : 'size: small');
    expect(worker.call('POST', { a: 1 }).body.result).toBe('size: small');

    expect(worker.call('PUT', { body: 'payload' }).body.result).toBe('echo: payload');
    expect(worker.call('PATCH', {}).body.result).toBe(10);
    expect(worker.call('DELETE', {}).status).toBe(404);
});

test('reports clear errors for unsupported syntax', () => {
    expect(() => compilePythonWorker('class Handler:\n    def foo(request) -> int:\n        return 1'))
        .toThrow(/must be an HTTP request method/);
    expect(() => compilePythonWorker('class Handler:\n    def GET(request) -> int:\n        return "x"'))
        .toThrow(/declared to return i32 but its body produces String/);
    expect(() => compilePythonWorker('class Handler:\n\tdef GET(request) -> int:\n\t\treturn 1'))
        .toThrow(/tabs are not supported/);
});
