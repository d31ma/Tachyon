// @ts-check
import { expect, test } from 'bun:test';
import { compileJavaScriptWorker, compileTypeScriptWorker } from '../../src/compiler/wasm/javascript-compiler.js';
import { compileRustWorker } from '../../src/compiler/wasm/rust-compiler.js';
import { TacWasmHost } from '../../src/runtime/tac-wasm-host.js';

/** @param {Uint8Array} bytes */
function loadWorker(bytes) {
    const host = TacWasmHost.instantiateSync(bytes, 'multi-lang-test');
    const encoder = new TextEncoder();
    return { call: host.call.bind(host), byteLen: (/** @type {unknown} */ v) => encoder.encode(JSON.stringify(v ?? null)).length };
}

const CASES = [
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
    pub fn OPTIONS(request: Request) -> String { return request.platform("runtime"); }
}
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
    /** @returns {string} */
    OPTIONS(request) { return request.platform("runtime"); }
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
    OPTIONS(request: Request): string { return request.platform("runtime"); }
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
        expect(worker.call('OPTIONS', {}).body.result).toBe('bun');
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
