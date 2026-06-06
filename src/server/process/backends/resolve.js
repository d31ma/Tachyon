// @ts-check
//
// Backend resolution coordinator.
//
// After routes are validated, decide which execution backend serves each
// handler file and register it. Preference order: in-house `wasm-compiled`
// (compiled/subset languages lowered by Tachyon's own compiler) → (later)
// `wasm-interpreter` (vendored Python/Ruby/PHP) → default `subprocess`. Called
// from the server's `configureRoutes` lifecycle; safe to call repeatedly (HMR).

import HandlerAdapter from '../handler-adapter.js';
import Router from '../../http/route-handler.js';
import { clearBackends } from './registry.js';
import * as wasmCompiled from './wasm-compiled.js';

/** @returns {string[]} Unique handler file paths across all routes/methods. */
function handlerPaths() {
    /** @type {Set<string>} */
    const paths = new Set();
    for (const methods of Object.values(Router.routeHandlers))
        for (const handlerPath of Object.values(methods))
            if (handlerPath) paths.add(handlerPath);
    return [...paths];
}

/**
 * Inspect every route handler and register its in-house execution backend.
 * Handlers with no in-house path keep the default subprocess backend.
 */
export function registerHandlerBackends() {
    for (const handlerPath of handlerPaths()) {
        const adapter = HandlerAdapter.resolve(handlerPath, []);
        if (!adapter)
            continue;
        // In-house compiled wasm (compiled + subset languages).
        if (wasmCompiled.tryRegister(handlerPath, adapter.language))
            continue;
        // Phase 2/3: wasm-interpreter for python/ruby/php goes here.
        // Otherwise the handler stays on the default subprocess backend.
    }
}

/** Drop all in-house backend registrations and warm caches (HMR reload). */
export function clearHandlerBackends() {
    clearBackends();
    wasmCompiled.clearHosts();
}
