// @ts-check
//
// In-house Go (handler subset) -> Wasm frontend.
//
// Normalizes `func (Handler) VERB(request Request) int|bool|string|json { ... }` into the
// shared C-style parser shape. This is a Tac-Go dialect, not the Go toolchain.

import { compileCppWorker } from './cpp-compiler.js';

/** @param {string} expr */
function inferDeclarationType(expr) {
    if (/\bjson\s*\(/.test(expr) || /\.json\s*\(/.test(expr)) return 'json';
    if (/\b(?:true|false)\b|[!=<>]=|&&|\|\|/.test(expr)) return 'bool';
    return /"/.test(expr) ? 'string' : 'int';
}

/**
 * Compile Tac-Go worker source into worker-ABI wasm bytes.
 * @param {string} source
 * @returns {Uint8Array}
 */
export function compileGoWorker(source) {
    const body = source
        .replace(/^\s*package\s+[A-Za-z_][A-Za-z0-9_]*\s*$/gm, '')
        .replace(/^\s*import\s+(?:\([^)]+\)|"[^"]+")\s*$/gm, '')
        .replace(/^\s*type\s+Handler\s+struct\s*\{\s*\}\s*$/gm, '')
        .replace(/\bfunc\s+(?:\(\s*Handler\s*\)\s*)?([A-Z][A-Z0-9_]*)\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s+Request\s*\)\s+(int|int8|int16|int32|uint|uint8|uint16|uint32|uintptr|bool|string|json)\s*\{/g, (_match, name, param, type) => {
            return `static ${type} ${name}(Request ${param}) {`;
        })
        .replace(/\bfor\s+([^{]+)\{/g, (_match, cond) => `while (${String(cond).trim()}) {`)
        .replace(/^(\s*)var\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([\s\S]*?);\s*$/gm, (_match, indent, name, expr) => {
            return `${indent}${inferDeclarationType(expr)} ${name} = ${expr};`;
        });
    return compileCppWorker(`class Handler { public:\n${body}\n};`);
}
