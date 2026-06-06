// @ts-check
import { afterEach, expect, test } from 'bun:test';
import path from 'path';
import HandlerAdapter from '../../../src/server/process/handler-adapter.js';
import { clearHosts, getResponse, tryRegister } from '../../../src/server/process/backends/wasm-compiled.js';
import { clearBackends, resolveBackend } from '../../../src/server/process/backends/registry.js';

const ROUTES = path.join(import.meta.dir, '../../../examples/server/routes/languages-wasm');
const DEMOS = [
    { lang: 'rust', file: 'rust/yon.rs' },
    { lang: 'cpp', file: 'cpp/yon.cpp' },
    { lang: 'csharp', file: 'csharp/yon.cs' },
];

afterEach(() => {
    clearBackends();
    clearHosts();
});

for (const { lang, file } of DEMOS) {
    test(`${lang} wasm demo route registers and serves in-process`, async () => {
        const handler = path.join(ROUTES, file);

        // HandlerAdapter recognises the class/struct Handler convention.
        const adapter = HandlerAdapter.resolve(handler, []);
        expect(adapter?.language).toBe(lang);

        // Tachyon compiles it to wasm in-house and routes it to the backend.
        expect(tryRegister(handler, lang)).toBe(true);
        expect(resolveBackend(handler)).toBe('wasm-compiled');

        // GET returns the request envelope byte length (> 0).
        const get = await getResponse(handler, { method: 'GET', body: '' });
        expect(get.status).toBe(200);
        expect(Number(get.body)).toBeGreaterThan(0);

        // POST echoes a body-derived summary string.
        const post = await getResponse(handler, { method: 'POST', body: 'hi' });
        expect(post.status).toBe(200);
        expect(post.body).toContain('received ');
        expect(post.body).toContain('hi');
    });
}
