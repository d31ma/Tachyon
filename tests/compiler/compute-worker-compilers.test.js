// @ts-check
import { expect, test } from 'bun:test';
import { compileCWorker } from '../../src/compiler/wasm/c-compiler.js';
import { compileCSharpWorker } from '../../src/compiler/wasm/csharp-compiler.js';
import { compileCppWorker } from '../../src/compiler/wasm/cpp-compiler.js';
import { compileGoWorker } from '../../src/compiler/wasm/go-compiler.js';
import { compileJavaScriptWorker, compileTypeScriptWorker } from '../../src/compiler/wasm/javascript-compiler.js';
import { compilePythonWorker } from '../../src/compiler/wasm/python-compiler.js';
import { compileRustWorker } from '../../src/compiler/wasm/rust-compiler.js';
import { compileZigWorker } from '../../src/compiler/wasm/zig-compiler.js';

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
        const b = writeText(typeof request?.body === 'string' ? request.body : JSON.stringify(request?.body ?? null));
        exports.call(m.ptr, m.len, r.ptr, r.len, b.ptr, b.len);
        return JSON.parse(decoder.decode(view().subarray(exports.output_ptr(), exports.output_ptr() + exports.output_len())));
    };
    return { call, byteLen: (/** @type {unknown} */ v) => encoder.encode(JSON.stringify(v ?? null)).length };
}

const CASES = [
    {
        language: 'c',
        compile: compileCWorker,
        source: `
unsigned int GET(Request request) { return request.len(); }
string POST(Request request) {
    int size = request.len();
    string tier = size > 20 ? "big" : "small";
    return "size: " + tier;
}
json PATCH(Request request) { return json(request.body()); }
bool DELETE(Request request) { return request.len() > 2 && true; }
`,
    },
    {
        language: 'cpp',
        compile: compileCppWorker,
        source: `
class Handler {
public:
    static uint32_t GET(Request request) { return request.len(); }
    static string POST(Request request) {
        int size = request.len();
        string tier = size > 20 ? "big" : "small";
        return "size: " + tier;
    }
    static json PATCH(Request request) { return json(request.body()); }
    static bool DELETE(Request request) { return request.len() > 2 && true; }
};
`,
    },
    {
        language: 'rust',
        compile: compileRustWorker,
        source: `
impl Handler {
    pub fn GET(request: Request) -> u32 { return request.len(); }
    pub fn POST(request: Request) -> String {
        let size = request.len();
        let tier = if size > 20 { "big" } else { "small" };
        return "size: " + tier;
    }
    pub fn PATCH(request: Request) -> Json { return json(request.body()); }
    pub fn DELETE(request: Request) -> bool { return request.len() > 2 && true; }
}
`,
    },
    {
        language: 'python',
        compile: compilePythonWorker,
        source: `
class Handler:
    def GET(request) -> int:
        return request.len()
    def POST(request) -> str:
        size = request.len()
        tier = "big" if size > 20 else "small"
        return "size: " + tier
    def PATCH(request) -> json:
        return json(request.body())
    def DELETE(request) -> bool:
        return request.len() > 2 and True
`,
    },
    {
        language: 'zig',
        compile: compileZigWorker,
        source: `
const Handler = struct {
    pub fn GET(request: Request) u32 { return request.len(); }
    pub fn POST(request: Request) string {
        const size = request.len();
        const tier = size > 20 ? "big" : "small";
        return "size: " + tier;
    }
    pub fn PATCH(request: Request) json { return json(request.body()); }
    pub fn DELETE(request: Request) bool { return request.len() > 2 && true; }
};
`,
    },
    {
        language: 'csharp',
        compile: compileCSharpWorker,
        source: `
class Handler {
    public static uint GET(Request request) { return request.len(); }
    public static string POST(Request request) {
        int size = request.len();
        string tier = size > 20 ? "big" : "small";
        return "size: " + tier;
    }
    public static Json PATCH(Request request) { return json(request.body()); }
    public static bool DELETE(Request request) { return request.len() > 2 && true; }
}
`,
    },
    {
        language: 'go',
        compile: compileGoWorker,
        source: `
package main
type Handler struct{}
func (Handler) GET(request Request) int32 { return request.len(); }
func (Handler) POST(request Request) string {
    var size = request.len();
    var tier = size > 20 ? "big" : "small";
    return "size: " + tier;
}
func (Handler) PATCH(request Request) json { return json(request.body()); }
func (Handler) DELETE(request Request) bool { return request.len() > 2 && true; }
`,
    },
    {
        language: 'javascript',
        compile: compileJavaScriptWorker,
        source: `
class Handler {
    GET(request) { return request.len(); }
    /** @returns {string} */
    POST(request) {
        const size = request.len();
        const tier = size > 20 ? "big" : "small";
        return "size: " + tier;
    }
    /** @returns {json} */
    PATCH(request) { return json(request.body()); }
    /** @returns {boolean} */
    DELETE(request) { return request.len() > 2 && true; }
}
`,
    },
    {
        language: 'typescript',
        compile: compileTypeScriptWorker,
        source: `
export default class Handler {
    GET(request: Request): number { return request.len(); }
    POST(request: Request): string {
        const size: number = request.len();
        const tier: string = size > 20 ? "big" : "small";
        return "size: " + tier;
    }
    PATCH(request: Request): Json { return json(request.body()); }
    DELETE(request: Request): boolean { return request.len() > 2 && true; }
}
`,
    },
];

for (const entry of CASES) {
    test(`Tac-${entry.language} frontend compiles to runnable worker wasm`, async () => {
        const worker = await loadWorker(entry.compile(entry.source));
        const request = { body: 'hello' };
        expect(worker.call('GET', request).body.result).toBe(worker.byteLen(request));
        expect(worker.call('POST', { hello: 'world wide web' }).body.result)
            .toBe(worker.byteLen({ hello: 'world wide web' }) > 20 ? 'size: big' : 'size: small');
        expect(worker.call('PATCH', { body: { language: entry.language, count: 2 } }).body.result)
            .toEqual({ language: entry.language, count: 2 });
        expect(worker.call('DELETE', { body: 'hello' }).body.result).toBe(true);
        expect(worker.call('HEAD', {}).status).toBe(404);
    });
}

test('Tac worker Json methods can return the full request envelope', async () => {
    const worker = await loadWorker(compileRustWorker(`
impl Handler {
    pub fn OPTIONS(request: Request) -> Json {
        request.json()
    }
}
`));
    expect(worker.call('OPTIONS', { body: { message: 'hello' }, headers: { accept: 'application/json' } }).body.result)
        .toEqual({ body: { message: 'hello' }, headers: { accept: 'application/json' } });
});
