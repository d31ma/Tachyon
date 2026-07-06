// @ts-check
import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import { compileNativeWorkerExecutable, rustAvailable } from '../../src/compiler/native/worker-compiler.js';

describe('Native worker compiler', () => {
    /** @type {string} */
    let tmpDir;

    beforeEach(async () => {
        tmpDir = await mkdtemp(path.join(tmpdir(), 'tachyon-native-worker-test-'));
    });

    afterEach(async () => {
        await rm(tmpDir, { recursive: true, force: true });
    });

    test('compiles a Rust worker to a native executable', { skip: !rustAvailable() }, async () => {
        const sourcePath = path.join(tmpDir, 'tac.rs');
        const outputPath = path.join(tmpDir, 'worker');
        await writeFile(sourcePath, `
impl Handler {
    pub fn POST(request: Request) -> i32 {
        request.len()
    }
}
`);

        await compileNativeWorkerExecutable(
            { sourcePath, route: 'test', provider: { extension: '.rs', language: 'rust', targets: ['macos'] } },
            outputPath,
            { target: 'macos' }
        );

        const proc = Bun.spawn({ cmd: [outputPath], stdin: 'pipe', stdout: 'pipe' });
        proc.stdin.write(JSON.stringify({ method: 'POST', request: { body: 'hello' } }));
        proc.stdin.end();
        const out = await new Response(proc.stdout).text();
        const response = JSON.parse(out);

        expect(response.status).toBe(200);
        expect(response.body.result).toBe(5);
        expect(response.headers['Content-Type']).toBe('application/json');
    });

    test('worker can return a Json body', { skip: !rustAvailable() }, async () => {
        const sourcePath = path.join(tmpDir, 'tac.rs');
        const outputPath = path.join(tmpDir, 'worker');
        await writeFile(sourcePath, `
impl Handler {
    pub fn GET(request: Request) -> Json {
        request.json()
    }
}
`);

        await compileNativeWorkerExecutable(
            { sourcePath, route: 'test', provider: { extension: '.rs', language: 'rust', targets: ['macos'] } },
            outputPath,
            { target: 'macos' }
        );

        const proc = Bun.spawn({ cmd: [outputPath], stdin: 'pipe', stdout: 'pipe' });
        proc.stdin.write(JSON.stringify({ method: 'GET', request: { body: { count: 2 } } }));
        proc.stdin.end();
        const out = await new Response(proc.stdout).text();
        const response = JSON.parse(out);

        expect(response.status).toBe(200);
        expect(response.body.result.method).toBe('GET');
        expect(response.body.result.body).toEqual({ count: 2 });
    });
});
