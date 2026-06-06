// @ts-check
import { expect, test } from 'bun:test';
import { compileRustWorker, tokenize, Parser } from '../../src/compiler/wasm/rust-compiler.js';

/**
 * Drive generated wasm through the exact Tac Worker ABI. `method` is the HTTP
 * request verb (as the fetch shadow sends it), which selects the handler.
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
// A yon-shaped worker handler compiled in-house to wasm. Methods are HTTP verbs.
impl Handler {
    pub fn GET(request: Request) -> i32 {
        request.len() * 2
    }

    pub fn PUT(request: Request) -> i32 {
        let n = request.len();
        if n > 20 { 1 } else { 0 }
    }

    pub fn POST(request: Request) -> i32 {
        let mut total = 0;
        let mut i = 0;
        while i < 5 {
            total = total + i;
            i = i + 1;
        }
        total
    }
}
`;

test('tokenizer skips comments/attributes and yields an EOF token', () => {
    const tokens = tokenize('#[no_mangle]\nimpl Handler {} // trailing');
    expect(tokens.at(-1)?.type).toBe('eof');
    expect(tokens.find((t) => t.value === 'impl')?.type).toBe('kw');
    expect(tokens.some((t) => t.value === 'no_mangle')).toBe(false);
});

test('parser builds a verb-named method list from the handler shape', () => {
    const program = new Parser(tokenize(SOURCE)).parseProgram();
    expect(program.methods.map((m) => m.name)).toEqual(['GET', 'PUT', 'POST']);
});

test('dispatches by HTTP verb and compiles arithmetic, if, and while to wasm', async () => {
    const worker = await loadWorker(compileRustWorker(SOURCE));

    const request = { method: 'POST', body: { text: 'hello world' } };
    const requestBytes = worker.byteLen(request);

    const get = worker.call('GET', request);
    expect(get.status).toBe(200);
    expect(get.body.method).toBe('GET');
    expect(get.body.result).toBe(requestBytes * 2);

    const big = worker.call('PUT', { hello: 'world wide web' });
    const small = worker.call('PUT', { a: 1 });
    expect(big.body.result).toBe(worker.byteLen({ hello: 'world wide web' }) > 20 ? 1 : 0);
    expect(small.body.result).toBe(0);

    expect(worker.call('POST', {}).body.result).toBe(10);
});

test('a verb without a handler method falls back to 404', async () => {
    const worker = await loadWorker(compileRustWorker(SOURCE));
    const response = worker.call('DELETE', {});
    expect(response.status).toBe(404);
    expect(response.body.error).toBe('unknown method');
});

test('rejects handler methods that are not HTTP request verbs', () => {
    expect(() => compileRustWorker('impl Handler { pub fn measure(request: Request) -> i32 { request.len() } }'))
        .toThrow(/must be an HTTP request method/);
});

test('reports clear errors for unsupported syntax', () => {
    expect(() => compileRustWorker('impl Handler { pub fn GET(request: Request) -> i32 { y } }'))
        .toThrow(/unknown variable 'y'/);
    expect(() => compileRustWorker('impl Handler { pub fn GET(request: Request) -> i32 { let a = 1; } }'))
        .toThrow(/must end with an expression/);
    expect(() => compileRustWorker('impl Handler { pub fn GET(request: Request) -> i32 { request.bytes() } }'))
        .toThrow(/request\.len\(\), request\.body\(\), and request\.json\(\) are available/);
});

const STRING_SOURCE = `
impl Handler {
    pub fn GET(request: Request) -> String {
        "bytes=" + request.len()
    }

    pub fn POST(request: Request) -> String {
        let label = if request.len() > 20 { "big" } else { "small" };
        "size is " + label
    }

    pub fn PUT(request: Request) -> String {
        "he said \\"hi\\" \\\\ done"
    }
}
`;

test('compiles String return types with concatenation and int coercion', async () => {
    const worker = await loadWorker(compileRustWorker(STRING_SOURCE));

    const request = { method: 'GET', body: 'x' };
    const get = worker.call('GET', request);
    expect(get.status).toBe(200);
    expect(typeof get.body.result).toBe('string');
    expect(get.body.result).toBe(`bytes=${worker.byteLen(request)}`);

    expect(worker.call('POST', { a: 1 }).body.result).toBe('size is small');
    expect(worker.call('POST', { hello: 'world wide web' }).body.result).toBe('size is big');
});

test('JSON-escapes string results so the response stays parseable', async () => {
    const worker = await loadWorker(compileRustWorker(STRING_SOURCE));
    // PUT returns: he said "hi" \ done  — quotes and backslash must be escaped.
    const put = worker.call('PUT', {});
    expect(put.body.result).toBe('he said "hi" \\ done');
});

test('rejects return-type mismatches between the declaration and the body', () => {
    expect(() => compileRustWorker('impl Handler { pub fn GET(request: Request) -> i32 { "nope" } }'))
        .toThrow(/declared to return i32 but its body produces String/);
    expect(() => compileRustWorker('impl Handler { pub fn GET(request: Request) -> String { 5 } }'))
        .toThrow(/declared to return String but its body produces i32/);
});

test('rejects arithmetic on string operands', () => {
    expect(() => compileRustWorker('impl Handler { pub fn GET(request: Request) -> i32 { "a" * 2 } }'))
        .toThrow(/operator '\*' requires i32 operands/);
});

test('request.body() exposes the request body as a string', async () => {
    const worker = await loadWorker(compileRustWorker(`
        impl Handler {
            pub fn POST(request: Request) -> String {
                "echo: " + request.body()
            }
        }
    `));
    // A string body is passed through raw (not JSON-quoted).
    expect(worker.call('POST', { body: 'hello world' }).body.result).toBe('echo: hello world');
    // A JSON-escapable body stays parseable in the response.
    expect(worker.call('POST', { body: 'a "quote" \\ slash' }).body.result).toBe('echo: a "quote" \\ slash');
});
