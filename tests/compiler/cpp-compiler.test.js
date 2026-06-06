// @ts-check
import { expect, test } from 'bun:test';
import { compileCppWorker, tokenize, Parser } from '../../src/compiler/wasm/cpp-compiler.js';

/**
 * Drive generated wasm through the exact Tac Worker ABI. `verb` is the HTTP
 * request method (as the fetch shadow sends it), which selects the handler.
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
#include <string>
// A yon-shaped Tac-C++ worker compiled in-house to wasm.
class Handler {
public:
    static int GET(Request request) {
        return request.len() * 2;
    }

    static string POST(const Request& request) {
        int n = request.len();
        string tier = n > 32 ? "big" : "small";
        return "size: " + tier;
    }

    static int PUT(Request request) {
        int total = 0;
        int i = 0;
        while (i < 5) {
            total = total + i;
            i = i + 1;
        }
        return total;
    }
};
`;

test('tokenizer skips preprocessor lines and comments', () => {
    const tokens = tokenize('#include <string>\nclass Handler {}; // tail');
    expect(tokens.at(-1)?.type).toBe('eof');
    expect(tokens.find((t) => t.value === 'class')?.type).toBe('kw');
    expect(tokens.some((t) => t.value === 'include')).toBe(false);
});

test('parser builds a verb-named method list from the class shape', () => {
    const program = new Parser(tokenize(SOURCE)).parseProgram();
    expect(program.methods.map((m) => m.name)).toEqual(['GET', 'POST', 'PUT']);
    expect(program.methods.map((m) => m.returnType)).toEqual(['i32', 'str', 'i32']);
});

test('dispatches by HTTP verb; compiles ternary, while, and string concat', async () => {
    const worker = await loadWorker(compileCppWorker(SOURCE));

    const request = { method: 'POST', body: { text: 'hi' } };
    const get = worker.call('GET', request);
    expect(get.status).toBe(200);
    expect(get.body.result).toBe(worker.byteLen(request) * 2);

    const big = worker.call('POST', { hello: 'world wide web' });
    const small = worker.call('POST', { a: 1 });
    expect(big.body.result).toBe(worker.byteLen({ hello: 'world wide web' }) > 32 ? 'size: big' : 'size: small');
    expect(small.body.result).toBe('size: small');

    expect(worker.call('PUT', {}).body.result).toBe(10);
});

test('an unknown verb falls back to 404', async () => {
    const worker = await loadWorker(compileCppWorker(SOURCE));
    expect(worker.call('DELETE', {}).status).toBe(404);
});

test('reports clear errors for unsupported syntax', () => {
    expect(() => compileCppWorker('class Handler { public: static int foo(Request request) { return 1; } };'))
        .toThrow(/must be an HTTP request method/);
    expect(() => compileCppWorker('class Handler { public: static int GET(Request request) { if (1) { return 1; } else { return 2; } } };'))
        .toThrow(/'if' statements are not supported/);
    expect(() => compileCppWorker('class Handler { public: static int GET(Request request) { return "x"; } };'))
        .toThrow(/declared to return i32 but its body produces String/);
});
