// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import { pathToFileURL } from 'url';
import Compiler from '../../src/compiler/index.js';
import { compileRustWorker } from '../../src/compiler/wasm/rust-compiler.js';

const originalFetch = globalThis.fetch;
const originalSelf = globalThis.self;

/** @type {Array<() => Promise<void> | void>} */
const cleanups = [];

afterEach(async () => {
    while (cleanups.length > 0)
        await cleanups.pop()?.();
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
    pub fn POST(request: Request) -> i32 { request.len() }
    pub fn PATCH(request: Request) -> Json { json(request.body()) }
}
`);
    await writeFile(workerPath, Compiler.createWorkerRuntimeSource('language/rust', schema));

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
    Object.defineProperty(globalThis, 'fetch', {
        configurable: true,
        writable: true,
        value: /** @type {typeof fetch} */ (async () => new Response(wasmBytes, {
            headers: { 'Content-Type': 'application/wasm' },
        })),
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
            request: { body: orderShape },
            200: {
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
            request: { body: orderShape },
            200: { method: '^PATCH$', result: orderShape },
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
            request: {
                body: {
                    'a+b': '^ok$',
                },
            },
            200: {
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
            request: { body: '^[\\s\\S]*$' },
            200: {
                method: '^POST$',
                result: '^0$',
            },
        },
    });

    await expect(runtime.call('POST', { body: 'non-empty' })).rejects.toThrow('schema mismatch');
});
