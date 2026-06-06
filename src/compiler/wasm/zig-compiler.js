// @ts-check
//
// In-house Zig (handler subset) -> Wasm frontend.
//
// Normalizes a tiny `const Handler = struct { pub fn VERB(...) ... }` Tac-Zig
// dialect into the shared C-style parser shape. No zig compiler is involved.

import { compileCppWorker } from './cpp-compiler.js';

/** @param {string} expr */
function inferDeclarationType(expr) {
    if (/\bjson\s*\(/.test(expr) || /\.json\s*\(/.test(expr)) return 'json';
    if (/\b(?:true|false)\b|[!=<>]=|&&|\|\|/.test(expr)) return 'bool';
    return /"/.test(expr) ? 'string' : 'int';
}

/**
 * Compile Tac-Zig worker source into worker-ABI wasm bytes.
 * @param {string} source
 * @returns {Uint8Array}
 */
export function compileZigWorker(source) {
    const normalized = source
        .replace(/const\s+Handler\s*=\s*struct\s*\{/g, 'class Handler { public:')
        .replace(/\bpub\s+fn\s+([A-Z][A-Z0-9_]*)\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*:\s*Request\s*\)\s*(i8|i16|i32|u8|u16|u32|isize|usize|bool|string|json)\s*\{/g, (_match, name, param, type) => {
            const returnType = type === 'string' ? 'string' : type === 'json' ? 'json' : type === 'bool' ? 'bool' : type;
            return `static ${returnType} ${name}(Request ${param}) {`;
        })
        .replace(/^(\s*)(?:var|const)\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([\s\S]*?);\s*$/gm, (_match, indent, name, expr) => {
            return `${indent}${inferDeclarationType(expr)} ${name} = ${expr};`;
        });
    return compileCppWorker(normalized);
}
