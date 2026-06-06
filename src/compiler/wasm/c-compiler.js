// @ts-check
//
// In-house C (handler subset) -> Wasm frontend.
//
// Tac-C uses top-level HTTP verb functions, then reuses the C++ subset parser
// after wrapping those functions in a synthetic Handler class. No C compiler,
// clang, Emscripten, or LLVM is involved.

import { compileCppWorker } from './cpp-compiler.js';

/**
 * Compile Tac-C worker source into worker-ABI wasm bytes.
 * @param {string} source
 * @returns {Uint8Array}
 */
export function compileCWorker(source) {
    const normalized = source
        .replace(/\b((?:(?:signed|unsigned)\s+)?(?:char|short|int|long)|bool|string|json|int8_t|uint8_t|int16_t|uint16_t|int32_t|uint32_t)\s+([A-Z][A-Z0-9_]*)\s*\(/g, 'static $1 $2(');
    return compileCppWorker(`class Handler { public:\n${normalized}\n};`);
}
