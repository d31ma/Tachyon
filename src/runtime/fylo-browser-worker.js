// @ts-check
//
// Fylo browser worker — boots the official @d31ma/fylo/browser OPFS engine in a
// Web Worker context for off-main-thread I/O. Replaces fylo-local-worker.js
// (deprecated) which used the hand-rolled OPFS filesystem adapter.
//
// The official @d31ma/fylo/browser package already provides a
// SharedWorker/DedicatedWorker runtime via its createBrowserClient({ worker: true })
// option. This worker entry point exists so the Tachyon compiler can bundle it
// as a known asset, used by the compiler's FYLO_LOCAL_WORKER_PATH constant.
import { createBrowserClient } from '@d31ma/fylo/browser';

const client = createBrowserClient({ storage: 'opfs', namespace: 'tachyon' });

// The official module handles its own message protocol internally when
// instantiated with worker: true. This file exists as a bundler target.
self.addEventListener('message', async (event) => {
    const { id, type, collection, method, args } = event.data || {};
    if (!id) return;

    try {
        if (type === 'collection') {
            const col = client.collection(collection);
            const result = await /** @type {Function} */ (/** @type {Record<string, any>} */ (col)[method])(...args);
            self.postMessage({ id, ok: true, result });
        } else {
            const result = await /** @type {Function} */ (/** @type {Record<string, any>} */ (client)[method])(...args);
            self.postMessage({ id, ok: true, result });
        }
    } catch (error) {
        self.postMessage({ id, ok: false, error: error instanceof Error ? error.message : String(error) });
    }
});
