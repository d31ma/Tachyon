// @ts-check
//
// In-house JavaScript/TypeScript (handler subset) -> Wasm frontend.
//
// This is not a general-purpose JS engine. It lets JS/TS authors write the same
// handler-shaped Tac Worker subset as the other language frontends, then lowers
// that subset through the shared normalized parser/codegen path to worker-ABI Wasm.

import { compileTacSubsetWorker } from './subset-compiler.js';

/** @typedef {{ name: string, param: string, returnType: string, body: string }} Method */

/** @param {string} type */
function mapReturnType(type) {
    const normalized = type.trim().replace(/[{}]/g, '').toLowerCase();
    if (['number', 'int', 'integer', 'i32', 'uint', 'u32'].includes(normalized)) return 'int';
    if (['boolean', 'bool'].includes(normalized)) return 'bool';
    if (['string', 'str'].includes(normalized)) return 'string';
    if (['json', 'object', 'array', 'unknown', 'any'].includes(normalized)) return 'json';
    return '';
}

/** @param {string} expr */
function inferDeclarationType(expr) {
    if (/\bjson\s*\(/.test(expr) || /\.json\s*\(/.test(expr)) return 'json';
    if (/\b(?:true|false)\b|[!=<>]=|&&|\|\|/.test(expr)) return 'bool';
    return /["'`]/.test(expr) ? 'string' : 'int';
}

/**
 * @param {string} source
 * @param {number} openIndex
 */
function findMatchingBrace(source, openIndex) {
    let depth = 0;
    let quote = '';
    for (let i = openIndex; i < source.length; i += 1) {
        const ch = source[i];
        if (quote) {
            if (ch === '\\') { i += 1; continue; }
            if (ch === quote) quote = '';
            continue;
        }
        if (ch === '"' || ch === "'" || ch === '`') { quote = ch; continue; }
        if (ch === '{') depth += 1;
        if (ch === '}') {
            depth -= 1;
            if (depth === 0) return i;
        }
    }
    throw new Error('Tac worker (javascript): unmatched class or method brace');
}

/** @param {string} source */
function extractHandlerClassBody(source) {
    const normalized = source
        .replace(/\bexport\s+default\s+/g, '')
        .replace(/\bexport\s+/g, '')
        .replace(/^\s*import\s+[^;]+;\s*$/gm, '');
    const classMatch = /\bclass\s+Handler\b[^{]*\{/.exec(normalized);
    if (!classMatch || classMatch.index === undefined)
        throw new Error('Tac worker (javascript): expected `class Handler { ... }`');
    const openIndex = normalized.indexOf('{', classMatch.index);
    const closeIndex = findMatchingBrace(normalized, openIndex);
    return normalized.slice(openIndex + 1, closeIndex);
}

/**
 * @param {string} text
 * @param {number} start
 */
function skipWhitespace(text, start) {
    let i = start;
    while (i < text.length) {
        if (/\s/.test(text[i])) { i += 1; continue; }
        if (text[i] === '/' && text[i + 1] === '/') {
            while (i < text.length && text[i] !== '\n') i += 1;
            continue;
        }
        break;
    }
    return i;
}

/**
 * @param {string} body
 * @param {number} methodStart
 */
function readLeadingJSDoc(body, methodStart) {
    const prefix = body.slice(0, methodStart);
    const match = prefix.match(/\/\*\*([\s\S]*?)\*\/\s*$/);
    return match?.[1] ?? '';
}

/** @param {string} params */
function normalizeParam(params) {
    const first = params.split(',').map((part) => part.trim()).filter(Boolean)[0] ?? 'request';
    const name = first.split('=')[0]?.split(':')[0]?.trim() || 'request';
    return /^[A-Za-z_][A-Za-z0-9_]*$/.test(name) ? name : 'request';
}

/** @param {string} raw */
function normalizeExpression(raw) {
    return raw
        .trim()
        .replace(/\btrue\b/g, 'true')
        .replace(/\bfalse\b/g, 'false')
        .replace(/!==/g, '!=')
        .replace(/===/g, '==')
        .replace(/`([^`$]*)`/g, (_match, text) => JSON.stringify(text));
}

/** @param {string} body */
function normalizeBody(body) {
    return body
        .replace(/!==/g, '!=')
        .replace(/===/g, '==')
        .replace(/`([^`$]*)`/g, (_match, text) => JSON.stringify(text))
        .replace(/\b(?:const|let|var)\s+([A-Za-z_][A-Za-z0-9_]*)(?:\s*:\s*([A-Za-z_][A-Za-z0-9_<>]*))?\s*=\s*([^;]+);/g, (_match, name, type, expr) => {
            const mapped = type ? mapReturnType(type) : '';
            return `${mapped || inferDeclarationType(expr)} ${name} = ${normalizeExpression(expr)};`;
        });
}

/**
 * @param {string} body
 * @param {string} jsdoc
 * @param {string} explicitType
 */
function resolveReturnType(body, jsdoc, explicitType) {
    const mappedExplicit = explicitType ? mapReturnType(explicitType) : '';
    if (mappedExplicit) return mappedExplicit;
    const returnsMatch = jsdoc.match(/@returns?\s+\{([^}]+)\}/);
    const mappedJSDoc = returnsMatch ? mapReturnType(returnsMatch[1]) : '';
    if (mappedJSDoc) return mappedJSDoc;
    const returnMatches = [...body.matchAll(/\breturn\s+([^;]+);/g)];
    const expr = returnMatches.at(-1)?.[1] ?? body.trim();
    return inferDeclarationType(expr);
}

/** @param {string} source */
function parseMethods(source) {
    const body = extractHandlerClassBody(source);
    /** @type {Method[]} */
    const methods = [];
    let i = 0;
    while (i < body.length) {
        i = skipWhitespace(body, i);
        if (i >= body.length) break;
        const methodStart = i;
        if (body.startsWith('/**', i)) {
            const close = body.indexOf('*/', i + 3);
            if (close === -1) throw new Error('Tac worker (javascript): unterminated JSDoc comment');
            i = skipWhitespace(body, close + 2);
        }
        if (body.startsWith('static ', i)) i = skipWhitespace(body, i + 'static '.length);
        if (body.startsWith('async ', i))
            throw new Error('Tac worker (javascript): async worker methods are not supported because Wasm calls are synchronous');
        const nameMatch = /^[A-Z][A-Z0-9_]*/.exec(body.slice(i));
        if (!nameMatch) {
            i += 1;
            continue;
        }
        const name = nameMatch[0];
        i += name.length;
        i = skipWhitespace(body, i);
        if (body[i] !== '(')
            throw new Error(`Tac worker (javascript): expected '(' after ${name}`);
        const paramsEnd = body.indexOf(')', i + 1);
        if (paramsEnd === -1)
            throw new Error(`Tac worker (javascript): expected ')' after ${name} parameters`);
        const param = normalizeParam(body.slice(i + 1, paramsEnd));
        i = skipWhitespace(body, paramsEnd + 1);
        let explicitType = '';
        if (body[i] === ':') {
            i += 1;
            const typeMatch = /^\s*([A-Za-z_][A-Za-z0-9_<>]*)/.exec(body.slice(i));
            if (!typeMatch)
                throw new Error(`Tac worker (typescript): expected return type after '${name}(...):'`);
            explicitType = typeMatch[1];
            i += typeMatch[0].length;
        }
        i = skipWhitespace(body, i);
        if (body[i] !== '{')
            throw new Error(`Tac worker (javascript): expected method body for ${name}`);
        const methodEnd = findMatchingBrace(body, i);
        const rawBody = body.slice(i + 1, methodEnd);
        const jsdoc = readLeadingJSDoc(body, methodStart);
        methods.push({
            name,
            param,
            returnType: resolveReturnType(rawBody, jsdoc, explicitType),
            body: normalizeBody(rawBody),
        });
        i = methodEnd + 1;
    }
    if (methods.length === 0)
        throw new Error('Tac worker (javascript): class Handler must declare at least one HTTP verb method');
    return methods;
}

/** @param {string} source */
function normalizeToSubset(source) {
    const methods = parseMethods(source);
    return `class Handler { public:\n${methods.map((method) => `static ${method.returnType} ${method.name}(Request ${method.param}) {${method.body}\n}`).join('\n')}\n};`;
}

/**
 * Compile Tac-JavaScript worker source into worker-ABI wasm bytes.
 * @param {string} source
 * @returns {Uint8Array}
 */
export function compileJavaScriptWorker(source) {
    return compileTacSubsetWorker(normalizeToSubset(source));
}

/**
 * Compile Tac-TypeScript worker source into worker-ABI wasm bytes.
 * @param {string} source
 * @returns {Uint8Array}
 */
export function compileTypeScriptWorker(source) {
    return compileJavaScriptWorker(source);
}
