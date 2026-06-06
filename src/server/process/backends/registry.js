// @ts-check
//
// Execution-backend registry.
//
// Maps a resolved handler file path to the backend that should serve its
// requests: the existing `subprocess` runner, the in-house `wasm-compiled`
// backend (subset languages lowered to wasm by Tachyon's own compiler), or a
// `wasm-interpreter` backend (a vendored interpreter compiled to wasm running
// the author's full source). Phase 0 registers nothing and every handler falls
// back to `subprocess`, preserving today's behavior exactly; later phases
// populate the map at route-validation / bundle time.
//
// Deliberately dependency-free so it can be imported from anywhere (HTTP layer,
// compiler, process pool) without risking an import cycle.

/** @typedef {'subprocess' | 'wasm-compiled' | 'wasm-interpreter'} BackendKind */

/** @type {Map<string, BackendKind>} */
const backendByHandler = new Map();

/**
 * Record the backend that should execute a handler file.
 * @param {string} handlerPath Absolute path to the handler (`yon.<ext>`) file.
 * @param {BackendKind} kind
 */
export function setBackend(handlerPath, kind) {
    backendByHandler.set(handlerPath, kind);
}

/**
 * Resolve the backend for a handler file. Defaults to the subprocess runner
 * (today's behavior) when nothing has been registered for the path.
 * @param {string} handlerPath
 * @returns {BackendKind}
 */
export function resolveBackend(handlerPath) {
    return backendByHandler.get(handlerPath) ?? 'subprocess';
}

/**
 * Whether a handler has an in-house (non-subprocess) backend registered.
 * @param {string} handlerPath
 * @returns {boolean}
 */
export function hasInHouseBackend(handlerPath) {
    const kind = backendByHandler.get(handlerPath);
    return kind === 'wasm-compiled' || kind === 'wasm-interpreter';
}

/** Drop all backend registrations (called before an HMR reload). */
export function clearBackends() {
    backendByHandler.clear();
}
