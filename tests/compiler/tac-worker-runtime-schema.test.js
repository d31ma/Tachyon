// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import { pathToFileURL } from 'url';
import Compiler from '../../src/compiler/index.js';
import { compileRustWorker } from '../../src/compiler/wasm/rust-compiler.js';
import { buildRuntimeModule } from '../../src/compiler/wasm/tac-wasm-compiler.js';

const originalFetch = globalThis.fetch;
const originalSelf = globalThis.self;
const originalNavigator = globalThis.navigator;
const originalDangerousCaps = process.env.TAC_DANGEROUS_CAPABILITIES;
const originalNativeCaps = process.env.TAC_NATIVE_CAPABILITIES;

/** @type {Array<() => Promise<void> | void>} */
const cleanups = [];

afterEach(async () => {
    while (cleanups.length > 0)
        await cleanups.pop()?.();
    if (originalDangerousCaps === undefined)
        delete process.env.TAC_DANGEROUS_CAPABILITIES;
    else
        process.env.TAC_DANGEROUS_CAPABILITIES = originalDangerousCaps;
    if (originalNativeCaps === undefined)
        delete process.env.TAC_NATIVE_CAPABILITIES;
    else
        process.env.TAC_NATIVE_CAPABILITIES = originalNativeCaps;
    Object.defineProperty(globalThis, 'fetch', {
        configurable: true,
        writable: true,
        value: originalFetch,
    });
    Object.defineProperty(globalThis, 'self', {
        configurable: true,
        writable: true,
        value: originalSelf,
    });
    Object.defineProperty(globalThis, 'navigator', {
        configurable: true,
        writable: true,
        value: originalNavigator,
    });
});

/**
 * @param {unknown} schema
 */
async function loadGeneratedWorkerRuntime(schema) {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-tac-worker-schema-'));
    cleanups.push(() => rm(root, { recursive: true, force: true }));
    const workerPath = path.join(root, 'tac.worker.js');
    const wasmBytes = compileRustWorker(`
impl Handler {
    pub fn GET(request: Request) -> String { request.platform("os") }
    pub fn POST(request: Request) -> i32 { request.len() }
    pub fn PATCH(request: Request) -> Json { json(request.body()) }
}
`);
    await writeFile(workerPath, Compiler.createWorkerRuntimeSource('language/rust', 'rs.wasm', schema));

    /** @type {unknown[]} */
    const messages = [];
    const fakeSelf = {
        /** @type {((event: { data: unknown }) => Promise<void>) | null} */
        onmessage: null,
        /** @param {unknown} message */
        postMessage(message) {
            messages.push(message);
        },
    };
    Object.defineProperty(globalThis, 'self', {
        configurable: true,
        writable: true,
        value: fakeSelf,
    });
    Object.defineProperty(globalThis, 'navigator', {
        configurable: true,
        writable: true,
        value: {
            platform: 'MacIntel',
            userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)',
            hardwareConcurrency: 8,
            language: 'en-CA',
            onLine: true,
            maxTouchPoints: 0,
        },
    });
    const runtimeBytes = buildRuntimeModule();
    Object.defineProperty(globalThis, 'fetch', {
        configurable: true,
        writable: true,
        value: /** @type {typeof fetch} */ (async (/** @type {Request | string | URL | undefined} */ input) => {
            if (!input) {
                throw new Error('fetch called without input');
            }
            const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
            if (url.includes('tac-runtime.wasm')) {
                return new Response(runtimeBytes, { headers: { 'Content-Type': 'application/wasm' } });
            }
            return new Response(wasmBytes, { headers: { 'Content-Type': 'application/wasm' } });
        }),
    });

    await import(`${pathToFileURL(workerPath).href}?t=${Date.now()}-${Math.random()}`);
    if (!fakeSelf.onmessage)
        throw new Error('generated Tac worker did not register an onmessage handler');

    return {
        /**
         * @param {string} method
         * @param {unknown} request
         */
        async call(method, request) {
            messages.length = 0;
            await fakeSelf.onmessage?.({ data: { id: 1, method, request } });
            const message = /** @type {{ ok: boolean, response?: unknown, error?: string }} */ (messages[0]);
            if (!message)
                throw new Error('generated Tac worker did not post a response');
            if (!message.ok)
                throw new Error(message.error || 'Tac worker call failed');
            return message.response;
        },
        /**
         * @param {string} method
         * @param {unknown} request
         * @param {unknown} value
         */
        async callWithNative(method, request, value) {
            messages.length = 0;
            const pendingCall = fakeSelf.onmessage?.({ data: { id: 1, method, request } });
            for (let attempt = 0; attempt < 25 && messages.length === 0; attempt += 1)
                await new Promise((resolve) => setTimeout(resolve, 0));
            const nativeRequest = /** @type {{ type: string, nativeId: number, capability: string, payload: unknown }} */ (messages[0]);
            if (!nativeRequest || nativeRequest.type !== 'tac:native-request')
                throw new Error('generated Tac worker did not post a native capability request');
            messages.length = 0;
            await fakeSelf.onmessage?.({
                data: {
                    type: 'tac:native-response',
                    nativeId: nativeRequest.nativeId,
                    ok: true,
                    value,
                },
            });
            await pendingCall;
            const message = /** @type {{ ok: boolean, response?: unknown, error?: string }} */ (messages[0]);
            if (!message)
                throw new Error('generated Tac worker did not post a native response');
            if (!message.ok)
                throw new Error(message.error || 'Tac worker call failed');
            return { nativeRequest, response: message.response };
        },
    };
}

const orderShape = {
    orderId: '^ORD-[0-9]+$',
    items: [{
        sku: '^[A-Z0-9-]+$',
        quantity: '^[1-9][0-9]*$',
    }],
    meta: {
        '^[a-z]+$': '^[0-9]+$',
    },
    'note?': '^.+$',
};

test('generated Tac worker runtime validates CHEX-style arrays, records, and nullable fields', async () => {
    const runtime = await loadGeneratedWorkerRuntime({
        PATCH: {
            payload: { body: orderShape },
            ok: {
                method: '^PATCH$',
                result: orderShape,
            },
        },
    });

    const body = {
        orderId: 'ORD-100',
        items: [{ sku: 'SKU-1', quantity: 2 }],
        meta: { batch: 42 },
        note: null,
    };
    const response = /** @type {{ body: { result: unknown } }} */ (await runtime.call('PATCH', { body }));

    expect(response.body.result).toEqual(body);
});

test('generated Tac worker runtime rejects unknown nested properties with CHEX strictness', async () => {
    const runtime = await loadGeneratedWorkerRuntime({
        PATCH: {
            payload: { body: orderShape },
            ok: { method: '^PATCH$', result: orderShape },
        },
    });

    await expect(runtime.call('PATCH', {
        body: {
            orderId: 'ORD-100',
            items: [{ sku: 'SKU-1', quantity: 2, color: 'blue' }],
            meta: { batch: 42 },
        },
    })).rejects.toThrow('unknown property');
});

test('generated Tac worker runtime treats regex-looking object keys as literal unless they are CHEX records', async () => {
    const runtime = await loadGeneratedWorkerRuntime({
        PATCH: {
            payload: {
                body: {
                    'a+b': '^ok$',
                },
            },
            ok: {
                method: '^PATCH$',
                result: {
                    'a+b': '^ok$',
                },
            },
        },
    });

    await expect(runtime.call('PATCH', { body: { aaab: 'ok' } })).rejects.toThrow('unknown property');
});

test('generated Tac worker runtime validates responses after Wasm execution', async () => {
    const runtime = await loadGeneratedWorkerRuntime({
        POST: {
            payload: { body: '^[\\s\\S]*$' },
            ok: {
                method: '^POST$',
                result: '^0$',
            },
        },
    });

    await expect(runtime.call('POST', { body: 'non-empty' })).rejects.toThrow('schema mismatch');
});

test('generated Tac worker runtime exposes curated platform facts only to Wasm workers', async () => {
    const runtime = await loadGeneratedWorkerRuntime({
        GET: {
            ok: {
                method: '^GET$',
                result: '^macos$',
            },
        },
    });

    const response = /** @type {{ body: { result: string } }} */ (await runtime.call('GET', {}));
    expect(response.body.result).toBe('macos');
});

test('generated Tac worker runtime validates clientError schemas', async () => {
    process.env.TAC_NATIVE_CAPABILITIES = 'app.info';
    const runtime = await loadGeneratedWorkerRuntime({
        PATCH: {
            payload: { body: '^[\\s\\S]*$' },
            ok: { method: '^PATCH$', result: { '$tacNative': { capability: '^.+$', 'payload?': '^[\\s\\S]*$', 'status?': '^[0-9]+$' } } },
            clientError: { message: '^not found$' },
        },
    });

    await expect(runtime.callWithNative('PATCH', {
        body: {
            $tacNative: {
                capability: 'app.info',
                payload: {},
                status: 404,
            },
        },
    }, { message: 'wrong' })).rejects.toThrow('schema mismatch');
});

test('generated Tac worker runtime validates serverError schemas', async () => {
    process.env.TAC_NATIVE_CAPABILITIES = 'app.info';
    const runtime = await loadGeneratedWorkerRuntime({
        PATCH: {
            payload: { body: '^[\\s\\S]*$' },
            ok: { method: '^PATCH$', result: { '$tacNative': { capability: '^.+$', 'payload?': '^[\\s\\S]*$', 'status?': '^[0-9]+$' } } },
            serverError: { message: '^server error$' },
        },
    });

    await expect(runtime.callWithNative('PATCH', {
        body: {
            $tacNative: {
                capability: 'app.info',
                payload: {},
                status: 500,
            },
        },
    }, { message: 'wrong' })).rejects.toThrow('schema mismatch');
});

test('generated Tac worker runtime brokers declared native capabilities', async () => {
    process.env.TAC_NATIVE_CAPABILITIES = 'app.info';
    const runtime = await loadGeneratedWorkerRuntime({});

    const { nativeRequest, response } = await runtime.callWithNative('PATCH', {
        body: {
            $tacNative: {
                capability: 'app.info',
                payload: {},
            },
        },
    }, { name: 'Native Test', runtime: 'test-host' });

    expect(nativeRequest.capability).toBe('app.info');
    expect(nativeRequest.payload).toEqual({});
    expect(response).toEqual({
        status: 200,
        headers: { 'Content-Type': 'application/json' },
        body: { name: 'Native Test', runtime: 'test-host' },
    });
});

test('generated Tac worker runtime authorizes capability family wildcards', async () => {
    process.env.TAC_NATIVE_CAPABILITIES = 'clipboard.*';
    const runtime = await loadGeneratedWorkerRuntime({});

    const { nativeRequest } = await runtime.callWithNative('PATCH', {
        body: {
            $tacNative: {
                capability: 'clipboard.readText',
                payload: { format: 'text/plain' },
            },
        },
    }, { text: 'copied from the host' });

    expect(nativeRequest.capability).toBe('clipboard.readText');
    expect(nativeRequest.payload).toEqual({ format: 'text/plain' });
});

test('capability family wildcards do not authorize unrelated capabilities', async () => {
    process.env.TAC_NATIVE_CAPABILITIES = 'clipboard.*';
    const runtime = await loadGeneratedWorkerRuntime({});

    await expect(runtime.call('PATCH', {
        body: {
            $tacNative: {
                capability: 'app.info',
                payload: {},
            },
        },
    })).rejects.toThrow('is not authorized; add it to TAC_NATIVE_CAPABILITIES');
});

test('generated Tac worker runtime denies undeclared native capabilities', async () => {
    delete process.env.TAC_NATIVE_CAPABILITIES;
    const runtime = await loadGeneratedWorkerRuntime({});

    await expect(runtime.call('PATCH', {
        body: {
            $tacNative: {
                capability: 'app.info',
                payload: {},
            },
        },
    })).rejects.toThrow('is not authorized; add it to TAC_NATIVE_CAPABILITIES');
});

test('generated Tac worker runtime denies raw OS capabilities absent from TAC_DANGEROUS_CAPABILITIES', async () => {
    delete process.env.TAC_DANGEROUS_CAPABILITIES;
    process.env.TAC_NATIVE_CAPABILITIES = 'fs.readText';
    const runtime = await loadGeneratedWorkerRuntime({});

    await expect(runtime.call('PATCH', {
        body: {
            $tacNative: {
                capability: 'fs.readText',
                payload: { path: '/tmp/example.txt' },
            },
        },
    })).rejects.toThrow('is not authorized; add it to TAC_DANGEROUS_CAPABILITIES');
});

test('generated Tac worker runtime permits raw OS capabilities authorized via TAC_DANGEROUS_CAPABILITIES', async () => {
    process.env.TAC_DANGEROUS_CAPABILITIES = 'fs.readText';
    process.env.TAC_NATIVE_CAPABILITIES = 'fs.readText';
    const runtime = await loadGeneratedWorkerRuntime({});

    const { nativeRequest, response } = await runtime.callWithNative('PATCH', {
        body: {
            $tacNative: {
                capability: 'fs.readText',
                payload: { path: '/tmp/example.txt' },
            },
        },
    }, { path: '/tmp/example.txt', text: 'hello' });

    expect(nativeRequest.capability).toBe('fs.readText');
    expect(nativeRequest.payload).toEqual({ path: '/tmp/example.txt' });
    expect(response).toEqual({
        status: 200,
        headers: { 'Content-Type': 'application/json' },
        body: { path: '/tmp/example.txt', text: 'hello' },
    });
});
