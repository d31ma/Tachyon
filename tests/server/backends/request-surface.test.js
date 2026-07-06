// @ts-check
import { test, expect } from 'bun:test';
import Compiler from '../../../src/compiler/index.js';
import { TacWasmHost } from '../../../src/runtime/tac-wasm-host.js';

// Each server compiled language reads request fields through the host-provided
// req_query/req_path/req_header imports (the host does the JSON work).
const HANDLERS = {
    rust: `struct Handler;
impl Handler {
    pub fn GET(request: Request) -> String { "code=" + request.query("code") }
    pub fn POST(request: Request) -> String { "id=" + request.path("id") + " ua=" + request.header("user-agent") }
}`,
};

for (const [language, source] of Object.entries(HANDLERS)) {
    test(`${language}: request.query/path/header resolve through host imports`, () => {
        const host = TacWasmHost.instantiateSync(Compiler.compileSubsetHandlerSource(language, source), `${language}-req`);

        expect(host.call('GET', { method: 'GET', query: { code: '404' } }).body.result).toBe('code=404');
        expect(host.call('POST', { method: 'POST', paths: { id: '42' }, headers: { 'user-agent': 'curl' } }).body.result)
            .toBe('id=42 ua=curl');
        // Missing key -> empty string (no trap).
        expect(host.call('GET', { method: 'GET', query: {} }).body.result).toBe('code=');
        expect(host.call('GET', { method: 'GET' }).body.result).toBe('code=');
    });
}

test('handlers that do not use request fields instantiate cleanly via TacWasmHost', async () => {
    const plain = `struct Handler; impl Handler { pub fn GET(request: Request) -> i32 { request.len() } }`;
    const host = TacWasmHost.instantiateSync(Compiler.compileSubsetHandlerSource('rust', plain), 'rust-plain');
    const response = host.call('GET', { method: 'GET' });
    expect(response.status).toBe(200);
});
