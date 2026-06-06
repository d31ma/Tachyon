// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { mkdtempSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import path from 'path';
import { clearHosts, getResponse, tryRegister } from '../../../src/server/process/backends/wasm-compiled.js';
import { clearBackends, resolveBackend } from '../../../src/server/process/backends/registry.js';

afterEach(() => {
    clearBackends();
    clearHosts();
});

/** @param {string} source @param {string} [ext] */
function writeHandler(source, ext = 'rs') {
    const dir = mkdtempSync(path.join(tmpdir(), 'yon-wasm-'));
    const file = path.join(dir, `yon.${ext}`);
    writeFileSync(file, source);
    return file;
}

const SUBSET_RUST = `struct Handler;
impl Handler {
    pub fn GET(request: Request) -> i32 { request.len() }
    pub fn POST(request: Request) -> String { "echo:" + request.body() }
}`;

test('registers and serves a subset Rust handler as in-house wasm', async () => {
    const handler = writeHandler(SUBSET_RUST);
    expect(tryRegister(handler, 'rust')).toBe(true);
    expect(resolveBackend(handler)).toBe('wasm-compiled');

    const get = await getResponse(handler, { method: 'GET', body: 'abcd' });
    expect(get.status).toBe(200);
    expect(Number(get.body)).toBeGreaterThan(0); // request.len() of the envelope

    const post = await getResponse(handler, { method: 'POST', body: 'hello' });
    expect(post.status).toBe(200);
    expect(post.body).toBe('echo:hello');

    // Unknown verb -> the dispatcher's 404 envelope is forwarded.
    const patch = await getResponse(handler, { method: 'PATCH', body: '' });
    expect(patch.status).toBe(404);
});

test('does not register when the source exceeds the subset (-> subprocess)', () => {
    const rich = `struct Handler;
impl Handler {
    pub fn GET(request: &YonJson) -> YonJson {
        match request.get("x").map(|v| v.as_string()).unwrap_or_default().as_str() {
            "a" => YonJson::object(vec![]),
            _ => YonJson::null(),
        }
    }
}`;
    const handler = writeHandler(rich);
    expect(tryRegister(handler, 'rust')).toBe(false);
    expect(resolveBackend(handler)).toBe('subprocess');
});

test('does not register a language without an in-house frontend', () => {
    const handler = writeHandler('class Handler { static Object GET(request) { return 1; } }', 'java');
    expect(tryRegister(handler, 'java')).toBe(false);
    expect(resolveBackend(handler)).toBe('subprocess');
});
