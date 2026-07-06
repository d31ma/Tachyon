// @ts-check
import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { mkdir, mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import Compiler from '../../src/compiler/index.js';
import Router from '../../src/server/http/route-handler.js';

describe('Multi-language workers', () => {
    /** @type {string} */
    let tmpDir;
    /** @type {string} */
    let workersDir;

    beforeEach(async () => {
        tmpDir = await mkdtemp(path.join(tmpdir(), 'tachyon-multi-worker-'));
        workersDir = path.join(tmpDir, 'workers');
        await mkdir(workersDir, { recursive: true });
        Object.defineProperty(Router, 'workersPath', { value: workersDir, configurable: true });
    });

    afterEach(async () => {
        await rm(tmpDir, { recursive: true, force: true });
    });

    test('two worker files with non-overlapping methods compile to separate wasm modules', async () => {
        const dir = path.join(workersDir, 'api', 'tasks');
        await mkdir(dir, { recursive: true });

        // Rust: DELETE only
        await writeFile(path.join(dir, 'tac.rs'), [
            'impl Handler {',
            '    pub fn DELETE(request: Request) -> u32 {',
            '        request.len()',
            '    }',
            '}',
        ].join('\n'));

        // TypeScript: GET and POST
        await writeFile(path.join(dir, 'tac.ts'), [
            'export default class Handler {',
            '    GET(request: Request): number {',
            '        return request.len();',
            '    }',
            '    POST(request: Request): number {',
            '        return request.len();',
            '    }',
            '}',
        ].join('\n'));

        await Compiler.bundleWorkers();

        const route = '/workers/api/tasks';
        expect(Router.reqRoutes[`${route}/rs.wasm`]).toBeDefined();
        expect(Router.reqRoutes[`${route}/ts.wasm`]).toBeDefined();
        expect(Router.reqRoutes[`${route}/tac.worker.js`]).toBeDefined();

        // Single-language .wasm and .worker.js should NOT exist
        expect(Router.reqRoutes[`${route}/tac.wasm`]).toBeUndefined();

        // The runtime should reference both modules
        const workerHandler = Router.reqRoutes[`${route}/tac.worker.js`]?.GET;
        expect(workerHandler).toBeDefined();
        const response = await workerHandler();
        const script = await response.text();
        expect(script).toContain('rs.wasm');
        expect(script).toContain('ts.wasm');
        expect(script).toContain('__tac_method_table__');
        expect(script).toContain('"GET":');
        expect(script).toContain('"POST":');
        expect(script).toContain('"DELETE":');
    });

    test('JavaScript worker methods participate in multi-module dispatch', async () => {
        const dir = path.join(workersDir, 'api', 'javascript-dispatch');
        await mkdir(dir, { recursive: true });

        await writeFile(path.join(dir, 'tac.js'), [
            'export default class Handler {',
            '    /** @returns {number} */',
            '    HEAD(request) {',
            '        return request.len();',
            '    }',
            '}',
        ].join('\n'));

        await writeFile(path.join(dir, 'tac.rs'), [
            'impl Handler {',
            '    pub fn GET(request: Request) -> u32 {',
            '        request.len()',
            '    }',
            '}',
        ].join('\n'));

        await Compiler.bundleWorkers();

        const route = '/workers/api/javascript-dispatch';
        const workerHandler = Router.reqRoutes[`${route}/tac.worker.js`]?.GET;
        expect(workerHandler).toBeDefined();
        const response = await workerHandler();
        const script = await response.text();
        expect(script).toContain('js.wasm');
        expect(script).toContain('"HEAD":');
        expect(script).toContain('function __tac_validate_request__');
        expect(script).toContain('function __tac_validate_response__');
        expect(script).toContain('this.modules.push({ runtimeExports, handlerExports: instance.exports });');
        expect(script).toContain('const module = this.requireModule(method);');
        expect(script).toContain('const methodInput = this.writeJson(runtimeExports, method);');
        expect(script).toContain('const bodyInput = this.writeText(runtimeExports, bodyText);');
        expect(script).toContain('req_platform: (keyPtr, keyLen) => this.writeStringValue(runtimeExports, __tac_platform_value__(');
    });

    test('overlapping methods across worker files are rejected', async () => {
        const dir = path.join(workersDir, 'api', 'conflict');
        await mkdir(dir, { recursive: true });

        await writeFile(path.join(dir, 'tac.rs'), [
            'impl Handler {',
            '    pub fn GET(request: Request) -> u32 {',
            '        42',
            '    }',
            '}',
        ].join('\n'));

        await writeFile(path.join(dir, 'tac.ts'), [
            'export default class Handler {',
            '    GET(request: Request): number {',
            '        return 99;',
            '    }',
            '}',
        ].join('\n'));

        await expect(Compiler.bundleWorkers()).rejects.toThrow('Method conflict');
    });

    test('single worker file produces <ext>.wasm', async () => {
        const dir = path.join(workersDir, 'solo');
        await mkdir(dir, { recursive: true });

        await writeFile(path.join(dir, 'tac.rs'), [
            'impl Handler {',
            '    pub fn GET(request: Request) -> u32 {',
            '        request.len()',
            '    }',
            '    pub fn POST(request: Request) -> u32 {',
            '        201',
            '    }',
            '}',
        ].join('\n'));

        await Compiler.bundleWorkers();

        const route = '/workers/solo';
        expect(Router.reqRoutes[`${route}/rs.wasm`]).toBeDefined();
        expect(Router.reqRoutes[`${route}/tac.worker.js`]).toBeDefined();
        // The old tac.wasm name is no longer used
        expect(Router.reqRoutes[`${route}/tac.wasm`]).toBeUndefined();
    });
});
