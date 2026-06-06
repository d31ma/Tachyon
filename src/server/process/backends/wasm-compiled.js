// @ts-check
//
// `wasm-compiled` execution backend.
//
// Serves a `yon.[ext]` route by running an in-house-compiled wasm module
// in-process — no subprocess, no language toolchain. At registration time the
// handler source is lowered to worker-ABI wasm by Tachyon's own compiler
// (`Compiler.compileSubsetHandlerSource`) and instantiated once; each request
// drives that warm module via `TacWasmHost`. Handlers whose source exceeds the
// supported subset fail to compile and are left on the subprocess backend.

import { readFileSync } from 'fs';
import Compiler from '../../../compiler/index.js';
import { TacWasmHost } from '../../../runtime/tac-wasm-host.js';
import logger from '../../observability/logger.js';
import { setBackend } from './registry.js';

const log = logger.child({ scope: 'wasm-compiled' });

/** @typedef {import('../../http/route-handler.js').RequestContext} RequestContext */
/** @typedef {{ status: number, body?: string }} RouteResponse */
/**
 * The per-request payload handed to a backend: the parsed request plus the
 * dispatch verb. Structurally compatible with both the HTTP layer's and the
 * server's `RequestPayload` typedefs (all fields optional).
 * @typedef {{ method?: string, headers?: Record<string, string>, paths?: Record<string, unknown>, body?: unknown, query?: Record<string, unknown> }} RequestStdin
 */

/** Warm, instantiated hosts keyed by absolute handler path. */
const hosts = new Map();

/**
 * Attempt to compile + instantiate a handler as in-house wasm. On success the
 * `wasm-compiled` backend is registered for the handler and its warm host is
 * cached; on failure (no frontend, or source beyond the subset) returns `false`
 * so the caller leaves the handler on the subprocess backend.
 * @param {string} handlerPath Absolute path to the `yon.<ext>` file.
 * @param {string} language
 * @returns {boolean}
 */
export function tryRegister(handlerPath, language) {
    if (!Compiler.subsetFrontends[language])
        return false;
    let source;
    try {
        source = readFileSync(handlerPath, 'utf8');
    }
    catch {
        return false;
    }
    try {
        const bytes = Compiler.compileSubsetHandlerSource(language, source);
        hosts.set(handlerPath, TacWasmHost.instantiateSync(bytes, handlerPath));
        setBackend(handlerPath, 'wasm-compiled');
        log.debug('Registered wasm-compiled handler', { handler: handlerPath, language });
        return true;
    }
    catch (error) {
        // Beyond the in-house subset — transparently fall back to subprocess.
        log.debug('Handler exceeds the in-house subset; keeping subprocess backend', {
            handler: handlerPath,
            language,
            detail: error instanceof Error ? error.message : String(error),
        });
        return false;
    }
}

/** Drop every warm host (called before an HMR reload). */
export function clearHosts() {
    hosts.clear();
}

/**
 * Map the worker-ABI response envelope (`{ status, body: { method, result } }`)
 * onto a server {@link RouteResponse}. The handler's `result` becomes the HTTP
 * body: strings are returned verbatim, everything else is JSON-encoded. The
 * 404 "unknown method" envelope (no `result`) is forwarded as-is.
 * @param {{ status?: number, body?: any }} envelope
 * @returns {RouteResponse}
 */
function toRouteResponse(envelope) {
    const status = typeof envelope?.status === 'number' ? envelope.status : 200;
    const body = envelope?.body;
    const payload = body && typeof body === 'object' && 'result' in body ? body.result : body;
    const text = typeof payload === 'string' ? payload : JSON.stringify(payload ?? null);
    return { status, body: text };
}

/**
 * @param {string} handler
 * @param {RequestStdin} stdin
 * @param {RequestContext} [_context]
 * @param {unknown} [_config]
 * @returns {Promise<RouteResponse>}
 */
export async function getResponse(handler, stdin, _context, _config) {
    const host = hosts.get(handler);
    if (!host) {
        return { status: 500, body: JSON.stringify({ detail: `wasm-compiled handler not loaded: ${handler}` }) };
    }
    const method = String(stdin.method || 'GET');
    try {
        return toRouteResponse(host.call(method, /** @type {Record<string, unknown>} */ (stdin)));
    }
    catch (error) {
        const detail = error instanceof Error ? error.message : String(error);
        log.error('wasm-compiled handler failed', { handler, method, detail });
        return { status: 500, body: JSON.stringify({ detail }) };
    }
}

/**
 * Streaming variant. Subset handlers return a single value, so this yields one
 * chunk — preserving the streaming code path without incremental output.
 * @param {string} handler
 * @param {RequestStdin} stdin
 * @param {RequestContext} [context]
 * @param {unknown} [config]
 * @returns {AsyncGenerator<RouteResponse>}
 */
export async function* getStreamResponse(handler, stdin, context, config) {
    yield await getResponse(handler, stdin, context, config);
}
