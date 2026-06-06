// @ts-check
import { afterEach, describe, expect, test } from 'bun:test';
import { fetch as tacFetch, resetTacWorkerPoolsForTest } from '../../src/runtime/tac-worker.js';

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const originalWorker = globalThis.Worker;

class FakeNativeWorker {
    /** @type {FakeNativeWorker[]} */
    static instances = [];
    /** @type {string | URL} */
    specifier;
    /** @type {WorkerOptions | undefined} */
    options;
    /** @type {unknown[]} */
    messages = [];
    /** @type {Map<string, Set<(event: any) => void>>} */
    listeners = new Map();
    terminated = false;

    /**
     * @param {string | URL} specifier
     * @param {WorkerOptions} [options]
     */
    constructor(specifier, options) {
        this.specifier = specifier;
        this.options = options;
        FakeNativeWorker.instances.push(this);
    }

    /** @param {unknown} message */
    postMessage(message) {
        this.messages.push(message);
    }

    terminate() {
        this.terminated = true;
    }

    /**
     * @param {string} type
     * @param {(event: any) => void} listener
     */
    addEventListener(type, listener) {
        const listeners = this.listeners.get(type) ?? new Set();
        listeners.add(listener);
        this.listeners.set(type, listeners);
    }

    /**
     * @param {string} type
     * @param {(event: any) => void} listener
     */
    removeEventListener(type, listener) {
        this.listeners.get(type)?.delete(listener);
    }

    /**
     * @param {string} type
     * @param {any} event
     */
    emit(type, event) {
        for (const listener of this.listeners.get(type) ?? [])
            listener(event);
    }
}

afterEach(() => {
    resetTacWorkerPoolsForTest();
    Object.defineProperty(globalThis, 'Worker', {
        configurable: true,
        writable: true,
        value: originalWorker,
    });
    FakeNativeWorker.instances = [];
});

function installFakeWorker() {
    Object.defineProperty(globalThis, 'Worker', {
        configurable: true,
        writable: true,
        value: FakeNativeWorker,
    });
}

describe('Tac-aware fetch', () => {
    test('routes tac:// URLs to a worker, sending the request verb as the method', async () => {
        installFakeWorker();
        const responsePromise = tacFetch('tac://language/rust', {
            method: 'post', // lower-case verb should be normalized to POST
            headers: { 'Content-Type': 'application/json' },
            body: { text: 'hi' },
        });
        await flush();

        const nativeWorker = FakeNativeWorker.instances.at(-1);
        if (!nativeWorker) throw new Error('expected a native worker to be created');
        expect(nativeWorker.specifier).toBe('/workers/language/rust/tac.worker.js');
        const message = /** @type {{ id: number, method: string, request: { body: unknown } }} */ (nativeWorker.messages.at(-1));
        expect(message.method).toBe('POST');
        expect(message.request.body).toEqual({ text: 'hi' });

        nativeWorker.emit('message', {
            data: { id: message.id, ok: true, response: { status: 200, body: { result: 42 } } },
        });

        const response = await responsePromise;
        expect(response.status).toBe(200);
        expect(await response.json()).toEqual({ result: 42 });
    });

    test('defaults to the GET verb when no method is provided', async () => {
        installFakeWorker();
        void tacFetch('tac://language/c').catch(() => {});
        await flush();
        const nativeWorker = FakeNativeWorker.instances.at(-1);
        if (!nativeWorker) throw new Error('expected a native worker to be created');
        const message = /** @type {{ method: string }} */ (nativeWorker.messages.at(-1));
        expect(message.method).toBe('GET');
    });

    test('reuses one route worker by default', async () => {
        installFakeWorker();
        void tacFetch('tac://language/go', { method: 'POST', body: 'one' }).catch(() => {});
        void tacFetch('tac://language/go', { method: 'POST', body: 'two' }).catch(() => {});
        await flush();

        expect(FakeNativeWorker.instances).toHaveLength(1);
        expect(FakeNativeWorker.instances[0].messages).toHaveLength(2);
    });

    test('spreads heavy tac calls across an opted-in worker pool', async () => {
        installFakeWorker();
        const first = tacFetch('tac://language/rust?pool=2', { method: 'POST', body: 'one' });
        const second = tacFetch('tac://language/rust?pool=2', { method: 'POST', body: 'two' });
        await flush();

        expect(FakeNativeWorker.instances).toHaveLength(2);
        const [left, right] = FakeNativeWorker.instances;
        expect(left.specifier).toBe('/workers/language/rust/tac.worker.js');
        expect(right.specifier).toBe('/workers/language/rust/tac.worker.js');
        expect(left.messages).toHaveLength(1);
        expect(right.messages).toHaveLength(1);

        const leftMessage = /** @type {{ id: number }} */ (left.messages[0]);
        const rightMessage = /** @type {{ id: number }} */ (right.messages[0]);
        left.emit('message', { data: { id: leftMessage.id, ok: true, response: { body: { result: 'left' } } } });
        right.emit('message', { data: { id: rightMessage.id, ok: true, response: { body: { result: 'right' } } } });

        expect(await (await first).json()).toEqual({ result: 'left' });
        expect(await (await second).json()).toEqual({ result: 'right' });
    });

    test('can grow a route worker pool from request metadata', async () => {
        installFakeWorker();
        void tacFetch('tac://language/python', { method: 'POST', body: 'one' }).catch(() => {});
        await flush();
        expect(FakeNativeWorker.instances).toHaveLength(1);

        void tacFetch('tac://language/python', {
            method: 'POST',
            headers: { 'X-Tac-Workers': '3' },
            body: 'two',
        }).catch(() => {});
        await flush();

        expect(FakeNativeWorker.instances).toHaveLength(3);
    });

    test('delegates non-tac URLs to the platform fetch', async () => {
        const original = globalThis.fetch;
        /** @type {{ input: unknown } | null} */
        let seen = null;
        globalThis.fetch = /** @type {typeof fetch} */ (async (input) => {
            seen = { input };
            return new Response('ok');
        });
        try {
            const response = await tacFetch('/api/data', { method: 'GET' });
            expect(seen?.input).toBe('/api/data');
            expect(await response.text()).toBe('ok');
        }
        finally {
            globalThis.fetch = original;
        }
    });

    test('rejects invalid tac protocol routes before creating a native worker', async () => {
        installFakeWorker();
        await expect(tacFetch('tac://language/_rust')).rejects.toThrow('must use lowercase alphanumeric');
        expect(FakeNativeWorker.instances).toHaveLength(0);
    });
});
