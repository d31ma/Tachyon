// @ts-check
//
// In-house C# (handler subset) -> Wasm frontend.
//
// Normalizes `class Handler { public static int VERB(...) { ... } }` into the
// shared C-style parser shape. This is a Tac-C# dialect, not Roslyn/.NET.

import { compileCppWorker } from './cpp-compiler.js';

/**
 * Compile Tac-C# worker source into worker-ABI wasm bytes.
 * @param {string} source
 * @returns {Uint8Array}
 */
export function compileCSharpWorker(source) {
    const normalized = source
        .replace(/\busing\s+[^;]+;/g, '')
        .replace(/\b(?:public|private|protected|internal)\s+(?=static\b)/g, '')
        .replace(/\bBoolean\b/g, 'bool')
        .replace(/\bString\b/g, 'string')
        .replace(/\bJson\b/g, 'json');
    return compileCppWorker(normalized);
}
