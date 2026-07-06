// @ts-check
import { afterEach, describe, expect, test } from 'bun:test';
import { mkdtemp, rm, writeFile } from 'fs/promises';
import path from 'path';
import { tmpdir } from 'os';
import { fetch as tacFetch, resetTacWorkerPoolsForTest } from '../../src/runtime/tac-worker.js';

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const originalWorker = globalThis.Worker;
const originalWindow = globalThis.window;
const originalWebkit = globalThis.webkit;
const originalDocument = globalThis.document;
const originalLocation = globalThis.location;
/** @type {string[]} */
const tempDirs = [];

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
    Reflect.deleteProperty(globalThis, '__tcNativeCapabilities__');
    Reflect.deleteProperty(globalThis, '__tcNativeBridge__');
    if (originalWindow === undefined)
        Reflect.deleteProperty(globalThis, 'window');
    else
        globalThis.window = originalWindow;
    if (originalWebkit === undefined)
        Reflect.deleteProperty(globalThis, 'webkit');
    else
        globalThis.webkit = originalWebkit;
    if (originalDocument === undefined)
        Reflect.deleteProperty(globalThis, 'document');
    else
        globalThis.document = originalDocument;
    if (originalLocation === undefined)
        Reflect.deleteProperty(globalThis, 'location');
    else
        globalThis.location = originalLocation;
    return Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
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
        const responsePromise = tacFetch('tac://language', {
            method: 'post', // lower-case verb should be normalized to POST
            headers: { 'Content-Type': 'application/json' },
            body: { text: 'hi' },
        });
        await flush();

        const nativeWorker = FakeNativeWorker.instances.at(-1);
        if (!nativeWorker) throw new Error('expected a native worker to be created');
        expect(nativeWorker.specifier).toBe('/workers/language/tac.worker.js');
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
        const first = tacFetch('tac://language?pool=2', { method: 'POST', body: 'one' });
        const second = tacFetch('tac://language?pool=2', { method: 'POST', body: 'two' });
        await flush();

        expect(FakeNativeWorker.instances).toHaveLength(2);
        const [left, right] = FakeNativeWorker.instances;
        expect(left.specifier).toBe('/workers/language/tac.worker.js');
        expect(right.specifier).toBe('/workers/language/tac.worker.js');
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
        void tacFetch('tac://language', { method: 'POST', body: 'one' }).catch(() => {});
        await flush();
        expect(FakeNativeWorker.instances).toHaveLength(1);

        void tacFetch('tac://language', {
            method: 'POST',
            headers: { 'X-Tac-Workers': '3' },
            body: 'two',
        }).catch(() => {});
        await flush();

        expect(FakeNativeWorker.instances).toHaveLength(3);
    });

    test('brokers declared native capability requests from workers', async () => {
        installFakeWorker();
        globalThis.__tcNativeCapabilities__ = {
            'app.info': async () => ({ name: 'Native Test', runtime: 'test-host' }),
        };

        void tacFetch('tac://language/native', { method: 'GET' }).catch(() => {});
        await flush();

        const nativeWorker = FakeNativeWorker.instances.at(-1);
        if (!nativeWorker) throw new Error('expected a native worker to be created');
        nativeWorker.emit('message', {
            data: {
                type: 'tac:native-request',
                nativeId: 7,
                capability: 'app.info',
                payload: {},
                source: 'language/native',
            },
        });
        await flush();

        expect(nativeWorker.messages.at(-1)).toEqual({
            type: 'tac:native-response',
            nativeId: 7,
            ok: true,
            value: { name: 'Native Test', runtime: 'test-host' },
        });
    });

    test('can broker raw filesystem reads when the host exposes OS APIs', async () => {
        installFakeWorker();
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-native-raw-'));
        tempDirs.push(root);
        const filePath = path.join(root, 'note.txt');
        await writeFile(filePath, 'raw access works', 'utf8');

        void tacFetch('tac://language/native', { method: 'PATCH' }).catch(() => {});
        await flush();

        const nativeWorker = FakeNativeWorker.instances.at(-1);
        if (!nativeWorker) throw new Error('expected a native worker to be created');
        nativeWorker.emit('message', {
            data: {
                type: 'tac:native-request',
                nativeId: 9,
                capability: 'fs.readText',
                payload: { path: filePath },
                source: 'language/native',
            },
        });
        // fs.readText awaits a dynamic import plus real file I/O, spanning
        // several async ticks — poll for the response rather than one flush.
        for (let i = 0; i < 200 && /** @type {any} */ (nativeWorker.messages.at(-1))?.type !== 'tac:native-response'; i++)
            await flush();

        expect(nativeWorker.messages.at(-1)).toEqual({
            type: 'tac:native-response',
            nativeId: 9,
            ok: true,
            value: { path: filePath, text: 'raw access works' },
        });
    });

    test('creates a page-world native bridge when a WebKit host is present without an injected helper', async () => {
        /** @type {unknown} */
        let nativeEnvelope = null;
        Object.defineProperty(globalThis, 'window', {
            configurable: true,
            writable: true,
            value: globalThis,
        });
        Object.defineProperty(globalThis, 'webkit', {
            configurable: true,
            writable: true,
            value: {
                messageHandlers: {
                    tachyon: {
                        /** @param {string} message */
                        postMessage(message) {
                            nativeEnvelope = JSON.parse(message);
                            queueMicrotask(() => {
                                const envelope = /** @type {{ id: number, payload: unknown }} */ (nativeEnvelope);
                                globalThis.__tcNativeBridge__.messageHandler(JSON.stringify({
                                    type: 'tac:native-response',
                                    id: envelope.id,
                                    ok: true,
                                    value: {
                                        status: 200,
                                        body: { native: true, payload: envelope.payload },
                                    },
                                }));
                            });
                        },
                    },
                },
            },
        });
        Object.defineProperty(globalThis, 'document', {
            configurable: true,
            writable: true,
            value: {
                baseURI: 'file:///tmp/Tachyon.app/Contents/Resources/index.html',
                /** @param {string} selector */
                querySelector(selector) {
                    if (selector === 'meta[name="tachyon-native-workers"]') {
                        return { getAttribute: () => 'desktop' };
                    }
                    return null;
                },
            },
        });

        const response = await tacFetch('tac://desktop', {
            method: 'POST',
            body: { value: 21 },
        });
        const envelope = /** @type {{ capability: string, payload: { route: string, method: string, request: { body: unknown } } }} */ (nativeEnvelope);

        expect(envelope.capability).toBe('tachyon.worker');
        expect(envelope.payload.route).toBe('desktop');
        expect(envelope.payload.method).toBe('POST');
        expect(envelope.payload.request.body).toEqual({ value: 21 });
        expect(globalThis.__tcNativeBridge__).toBeDefined();
        expect(await response.json()).toEqual({
            native: true,
            payload: {
                route: 'desktop',
                method: 'POST',
                request: {
                    method: 'POST',
                    headers: {},
                    body: { value: 21 },
                },
            },
        });
    });

    test('resolves allowlisted capabilities returned by native executable workers', async () => {
        Object.defineProperty(globalThis, 'window', {
            configurable: true,
            writable: true,
            value: globalThis,
        });
        Object.defineProperty(globalThis, 'document', {
            configurable: true,
            writable: true,
            value: {
                baseURI: 'file:///tmp/Tachyon.app/Contents/Resources/index.html',
                /** @param {string} selector */
                querySelector(selector) {
                    if (selector === 'meta[name="tachyon-native-workers"]')
                        return { getAttribute: () => 'desktop' };
                    if (selector === 'meta[name="tachyon-native-capabilities"]')
                        return { getAttribute: () => 'app.info' };
                    return null;
                },
            },
        });
        /** @type {any} */ (globalThis).__tcNativeCapabilities__ = {
            'app.info': async () => ({ name: 'Fixture App', runtime: 'test-host' }),
        };
        /** @type {any} */ (globalThis).__tcNativeBridge__ = {
            /** @param {string} capability */
            async invoke(capability) {
                expect(capability).toBe('tachyon.worker');
                return {
                    status: 200,
                    body: {
                        result: {
                            $tacNative: {
                                capability: 'app.info',
                                payload: {},
                            },
                        },
                    },
                };
            },
        };

        try {
            const response = await tacFetch('tac://desktop', { method: 'POST' });
            const body = await response.json();

            expect(body.name).toBe('Fixture App');
            expect(body.runtime).toBe('test-host');
        } finally {
            delete /** @type {any} */ (globalThis).__tcNativeCapabilities__;
        }
    });

    test('falls back to browser workers for native-host routes not listed as native executables', async () => {
        installFakeWorker();
        Object.defineProperty(globalThis, 'window', {
            configurable: true,
            writable: true,
            value: globalThis,
        });
        Object.defineProperty(globalThis, 'webkit', {
            configurable: true,
            writable: true,
            value: {
                messageHandlers: {
                    tachyon: { postMessage() { throw new Error('should not call native bridge'); } },
                },
            },
        });
        Object.defineProperty(globalThis, 'location', {
            configurable: true,
            writable: true,
            value: { protocol: 'file:' },
        });
        Object.defineProperty(globalThis, 'document', {
            configurable: true,
            writable: true,
            value: {
                baseURI: 'file:///tmp/Tachyon.app/Contents/Resources/index.html',
                /** @param {string} selector */
                querySelector(selector) {
                    if (selector === 'meta[name="tachyon-native-workers"]') {
                        return { getAttribute: () => 'desktop,native' };
                    }
                    return null;
                },
            },
        });

        const responsePromise = tacFetch('tac://language', { method: 'POST', body: 'hello' });
        await flush();

        const nativeWorker = FakeNativeWorker.instances.at(-1);
        if (!nativeWorker) throw new Error('expected a browser worker fallback to be created');
        expect(nativeWorker.specifier).toBe('file:///tmp/Tachyon.app/Contents/Resources/workers/language/tac.worker.js');
        const message = /** @type {{ id: number, method: string, request: { body: unknown } }} */ (nativeWorker.messages.at(-1));
        expect(message.method).toBe('POST');
        nativeWorker.emit('message', {
            data: { id: message.id, ok: true, response: { status: 200, body: { result: 'wasm fallback' } } },
        });
        expect(await (await responsePromise).json()).toEqual({ result: 'wasm fallback' });
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
