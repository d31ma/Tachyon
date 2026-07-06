// @ts-check
//
// Dedicated browser worker for Tac's local FYLO persistence path.
//
// The worker owns OPFS access and keeps storage/query work off the UI thread.
// It is also the only context where the high-performance synchronous OPFS API
// (createSyncAccessHandle) is permitted, which the engine uses for fast,
// durable, in-place writes. The pure-JS engine runs natively here behind a
// message protocol — no WebAssembly or external toolchain involved.

import { FyloLocalEngine } from './fylo-local.js';

const engine = new FyloLocalEngine();
/** @type {Map<string, () => void>} */
const subscriptions = new Map();

/**
 * @param {string} scope
 * @param {string} collection
 */
function keyFor(scope, collection) {
    return `${scope}:${collection}`;
}

/**
 * @param {unknown} value
 * @returns {Record<string, unknown>}
 */
function objectPayload(value) {
    return value && typeof value === 'object' ? /** @type {Record<string, unknown>} */ (value) : {};
}

/**
 * @param {Record<string, unknown>} message
 */
async function dispatch(message) {
    const type = String(message.type || '');
    const scope = String(message.scope || '');
    const collection = String(message.collection || '');
    switch (type) {
        case 'available':
            return engine.available();
        case 'find':
            return engine.find(scope, collection, objectPayload(message.query));
        case 'get':
            return engine.get(scope, collection, String(message.docId || ''));
        case 'ingestFindResult':
            return engine.ingestFindResult(scope, collection, message.payload);
        case 'ingestGetResult':
            return engine.ingestGetResult(scope, collection, message.payload);
        case 'put':
            return engine.put(
                scope,
                collection,
                String(message.docId || ''),
                message.doc,
                /** @type {'snapshot' | 'create' | 'put' | 'patch'} */ (message.op || 'put'),
            );
        case 'delete':
            return engine.delete(scope, collection, String(message.docId || ''));
        case 'subscribe': {
            const key = keyFor(scope, collection);
            if (!subscriptions.has(key)) {
                subscriptions.set(key, engine.subscribe(scope, collection, () => {
                    postMessage({ type: 'notify', scope, collection });
                }));
            }
            return true;
        }
        case 'unsubscribe': {
            const key = keyFor(scope, collection);
            subscriptions.get(key)?.();
            subscriptions.delete(key);
            return true;
        }
        default:
            throw new Error(`Unknown FYLO local worker request '${type}'`);
    }
}

self.onmessage = async (event) => {
    const message = objectPayload(event.data);
    const id = Number(message.id || 0);
    try {
        const result = await dispatch(message);
        postMessage({ id, ok: true, result });
    } catch (error) {
        postMessage({
            id,
            ok: false,
            error: error instanceof Error ? error.message : String(error),
        });
    }
};

