// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { FyloLocalEngine, resetFyloLocalEngineForTest } from '../../src/runtime/fylo-local.js';

const previousWindow = globalThis.window;
const previousNavigator = globalThis.navigator;

afterEach(() => {
    resetFyloLocalEngineForTest();
    if (previousWindow === undefined)
        Reflect.deleteProperty(globalThis, 'window');
    else
        Object.defineProperty(globalThis, 'window', { configurable: true, writable: true, value: previousWindow });
    Object.defineProperty(globalThis, 'navigator', { configurable: true, writable: true, value: previousNavigator });
});

function installFakeOpfs() {
    const files = new Map();
    const directory = {
        /** @param {string} _name */
        async getDirectoryHandle(_name) {
            return directory;
        },
        /** @param {string} name */
        async getFileHandle(name) {
            return {
                async getFile() {
                    return new Blob([files.get(name) ?? '']);
                },
                async createWritable() {
                    return {
                        /** @param {unknown} value */
                        async write(value) {
                            files.set(name, String(value));
                        },
                        async close() {},
                    };
                },
            };
        },
    };
    Object.defineProperty(globalThis, 'window', { configurable: true, writable: true, value: {} });
    Object.defineProperty(globalThis, 'navigator', {
        configurable: true,
        writable: true,
        value: {
            storage: {
                async getDirectory() {
                    return directory;
                },
            },
        },
    });
    return files;
}

test('FyloLocalEngine persists documents through OPFS when available', async () => {
    const files = installFakeOpfs();
    const engine = new FyloLocalEngine();

    await engine.put('anon', 'users', 'u1', { name: 'Ada', role: 'admin' }, 'put');
    const result = await engine.find('anon', 'users', { role: 'eq.admin' });

    expect(result.source).toBe('opfs');
    expect(result.docs).toEqual([{ id: 'u1', doc: { name: 'Ada', role: 'admin' } }]);
    expect([...files.values()].join('\n')).toContain('"u1"');
});

/**
 * Simulate a Worker context (no `window`, but `self` + `postMessage`) exposing
 * the synchronous OPFS access-handle API.
 */
function installFakeSyncOpfs() {
    const files = new Map();
    const stats = { writableCreated: 0, flushes: 0 };
    /** @param {string} name */
    const makeHandle = (name) => ({
        async createWritable() {
            stats.writableCreated += 1;
            throw new Error('async createWritable should not be used in a worker');
        },
        async createSyncAccessHandle() {
            let bytes = new TextEncoder().encode(files.get(name) ?? '');
            return {
                getSize: () => bytes.length,
                /** @param {Uint8Array} target @param {{ at?: number }} [opts] */
                read: (target, opts = {}) => {
                    const at = opts.at ?? 0;
                    const slice = bytes.subarray(at, at + target.length);
                    target.set(slice);
                    return slice.length;
                },
                /** @param {Uint8Array} source @param {{ at?: number }} [opts] */
                write: (source, opts = {}) => {
                    const incoming = new Uint8Array(source);
                    if ((opts.at ?? 0) === 0) bytes = incoming;
                    files.set(name, new TextDecoder().decode(bytes));
                    return incoming.length;
                },
                /** @param {number} size */
                truncate: (size) => { bytes = bytes.subarray(0, size); },
                flush: () => { stats.flushes += 1; },
                close: () => {},
            };
        },
    });
    const directory = {
        async getDirectoryHandle() { return directory; },
        /** @param {string} name */
        async getFileHandle(name) { return makeHandle(name); },
    };
    Reflect.deleteProperty(globalThis, 'window');
    Object.defineProperty(globalThis, 'self', { configurable: true, writable: true, value: globalThis });
    Object.defineProperty(globalThis, 'postMessage', { configurable: true, writable: true, value: () => {} });
    Object.defineProperty(globalThis, 'navigator', {
        configurable: true,
        writable: true,
        value: { storage: { async getDirectory() { return directory; } } },
    });
    return { files, stats };
}

test('FyloLocalEngine uses synchronous, flushed OPFS access handles in a worker', async () => {
    const previousSelf = /** @type {Record<string, unknown>} */ (globalThis).self;
    const previousPostMessage = /** @type {Record<string, unknown>} */ (globalThis).postMessage;
    const { files, stats } = installFakeSyncOpfs();
    try {
        const engine = new FyloLocalEngine();
        await engine.put('anon', 'users', 'u1', { name: 'Grace' }, 'put');
        const result = await engine.find('anon', 'users', {});

        expect(result.source).toBe('opfs');
        expect(result.docs).toEqual([{ id: 'u1', doc: { name: 'Grace' } }]);
        expect(stats.writableCreated).toBe(0); // sync handle used, not createWritable
        expect(stats.flushes).toBeGreaterThan(0); // durable: flush() was called
        expect([...files.values()].join('')).toContain('"Grace"');
    }
    finally {
        if (previousSelf === undefined) Reflect.deleteProperty(globalThis, 'self');
        else Object.defineProperty(globalThis, 'self', { configurable: true, writable: true, value: previousSelf });
        if (previousPostMessage === undefined) Reflect.deleteProperty(globalThis, 'postMessage');
        else Object.defineProperty(globalThis, 'postMessage', { configurable: true, writable: true, value: previousPostMessage });
    }
});
