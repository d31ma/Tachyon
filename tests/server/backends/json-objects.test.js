// @ts-check
import { test, expect } from 'bun:test';
import Compiler from '../../../src/compiler/index.js';
import { TacWasmHost } from '../../../src/runtime/tac-wasm-host.js';

const HANDLERS = {
    rust: `struct Handler;
impl Handler {
    pub fn GET(request: Request) -> Json {
        { "name": request.query("name"), "bytes": request.len(), "ok": true, "neg": 0 - 7 }
    }
    pub fn POST(request: Request) -> Json { {} }
}`,
};

for (const [language, source] of Object.entries(HANDLERS)) {
    test(`${language}: returns a structured JSON object built in-house`, () => {
        const host = TacWasmHost.instantiateSync(Compiler.compileSubsetHandlerSource(language, source), `${language}-obj`);

        const r = host.call('GET', { method: 'GET', query: { name: 'Ada' } }).body.result;
        expect(r.name).toBe('Ada');
        expect(typeof r.bytes).toBe('number');
        expect(r.ok).toBe(true);
        expect(r.neg).toBe(-7);

        // String values are quoted + escaped so the response stays valid JSON.
        const escaped = host.call('GET', { method: 'GET', query: { name: 'a"b\\c\nd' } }).body.result;
        expect(escaped.name).toBe('a"b\\c\nd');

        // Empty object literal.
        expect(host.call('POST', { method: 'POST' }).body.result).toEqual({});
    });
}
