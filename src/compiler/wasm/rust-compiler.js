// @ts-check
//
// In-house Rust (handler subset) -> Wasm frontend.
//
// Parses a `yon`-shaped worker source - `impl Handler { pub fn VERB(request:
// Request) -> i32|bool|String|Json { ... } }` - into the shared handler AST. Type
// checking, Wasm codegen, and the verb dispatcher all live in
// `tac-handler-codegen.js`, so this file is purely a lexer + parser. No
// rustc/LLVM is involved.
//
// Supported subset: i32-compatible integer aliases + bool + String/Json
// params/locals, `let`/`let mut`, assignment, arithmetic (`+ - * / %`),
// comparisons (`== != < <= > >=`), logical operators (`! && ||`), `if/else`
// expressions, `while` loops, string literals + concatenation,
// `request.len()` / `request.body()` / `request.json()`, `json(stringExpr)`,
// and a trailing expression (or final `return`) as the method result. Anything
// outside the subset raises a clear compile error.

import { compileHandlerProgram } from './tac-handler-codegen.js';

/** @typedef {Record<string, any>} Node */
/** @typedef {{ type: 'ident' | 'int' | 'str' | 'kw' | 'punct' | 'eof', value: string, line: number }} Token */

const INTEGER_TYPES = new Set(['i8', 'i16', 'i32', 'u8', 'u16', 'u32', 'isize', 'usize']);
const KEYWORDS = new Set(['impl', 'pub', 'fn', 'let', 'mut', 'if', 'else', 'while', 'return', 'bool', 'true', 'false', ...INTEGER_TYPES]);
const PUNCT2 = ['->', '==', '!=', '<=', '>=', '&&', '||'];
const PUNCT1 = ['{', '}', '(', ')', '[', ']', ';', ',', ':', '.', '=', '<', '>', '+', '-', '*', '/', '%', '&', '!'];

/**
 * Tokenize Rust-subset source.
 * @param {string} source
 * @returns {Token[]}
 */
export function tokenize(source) {
    /** @type {Token[]} */
    const tokens = [];
    let i = 0;
    let line = 1;
    const isIdentStart = (/** @type {string} */ c) => /[A-Za-z_]/.test(c);
    const isIdent = (/** @type {string} */ c) => /[A-Za-z0-9_]/.test(c);
    const isDigit = (/** @type {string} */ c) => c >= '0' && c <= '9';

    while (i < source.length) {
        const c = source[i];
        if (c === '\n') { line += 1; i += 1; continue; }
        if (c === ' ' || c === '\t' || c === '\r') { i += 1; continue; }
        // Line comment
        if (c === '/' && source[i + 1] === '/') {
            while (i < source.length && source[i] !== '\n') i += 1;
            continue;
        }
        // Block comment
        if (c === '/' && source[i + 1] === '*') {
            i += 2;
            while (i < source.length && !(source[i] === '*' && source[i + 1] === '/')) {
                if (source[i] === '\n') line += 1;
                i += 1;
            }
            i += 2;
            continue;
        }
        // Attribute `#[ ... ]` - skip wholesale (e.g. #[no_mangle]).
        if (c === '#' && source[i + 1] === '[') {
            let depth = 0;
            i += 1;
            do {
                if (source[i] === '[') depth += 1;
                else if (source[i] === ']') depth -= 1;
                else if (source[i] === '\n') line += 1;
                i += 1;
            } while (i < source.length && depth > 0);
            continue;
        }
        // String literal: "..." with \" \\ \n \t \r escapes.
        if (c === '"') {
            i += 1;
            let value = '';
            while (i < source.length && source[i] !== '"') {
                if (source[i] === '\\') {
                    const next = source[i + 1];
                    value += next === 'n' ? '\n' : next === 't' ? '\t' : next === 'r' ? '\r' : next ?? '';
                    i += 2;
                }
                else {
                    if (source[i] === '\n') line += 1;
                    value += source[i];
                    i += 1;
                }
            }
            if (source[i] !== '"')
                throw new Error(`Tac worker (rust): unterminated string literal on line ${line}`);
            i += 1;
            tokens.push({ type: 'str', value, line });
            continue;
        }
        if (isIdentStart(c)) {
            let start = i;
            while (i < source.length && isIdent(source[i])) i += 1;
            const value = source.slice(start, i);
            tokens.push({ type: KEYWORDS.has(value) ? 'kw' : 'ident', value, line });
            continue;
        }
        if (isDigit(c)) {
            let start = i;
            while (i < source.length && (isDigit(source[i]) || source[i] === '_')) i += 1;
            tokens.push({ type: 'int', value: source.slice(start, i).replace(/_/g, ''), line });
            continue;
        }
        const two = source.slice(i, i + 2);
        if (PUNCT2.includes(two)) { tokens.push({ type: 'punct', value: two, line }); i += 2; continue; }
        if (PUNCT1.includes(c)) { tokens.push({ type: 'punct', value: c, line }); i += 1; continue; }
        throw new Error(`Tac worker (rust): unexpected character '${c}' on line ${line}`);
    }
    tokens.push({ type: 'eof', value: '', line });
    return tokens;
}

/** Recursive-descent parser for the Rust handler subset. */
export class Parser {
    /** @param {Token[]} tokens */
    constructor(tokens) {
        this.tokens = tokens;
        this.pos = 0;
    }

    /** @returns {Token} */
    peek() { return this.tokens[this.pos]; }
    /** @returns {Token} */
    next() { return this.tokens[this.pos++]; }
    /** @param {string} value */
    isPunct(value) { const t = this.peek(); return t.type === 'punct' && t.value === value; }
    /** @param {string} value */
    isKw(value) { const t = this.peek(); return t.type === 'kw' && t.value === value; }

    /** @param {string} value */
    expectPunct(value) {
        const t = this.next();
        if (t.type !== 'punct' || t.value !== value)
            throw new Error(`Tac worker (rust): expected '${value}' but found '${t.value || 'end of input'}' on line ${t.line}`);
    }

    /** @param {string} value */
    expectKw(value) {
        const t = this.next();
        if (t.type !== 'kw' || t.value !== value)
            throw new Error(`Tac worker (rust): expected '${value}' but found '${t.value || 'end of input'}' on line ${t.line}`);
    }

    /** @returns {string} */
    expectIdent() {
        const t = this.next();
        if (t.type !== 'ident')
            throw new Error(`Tac worker (rust): expected an identifier but found '${t.value || 'end of input'}' on line ${t.line}`);
        return t.value;
    }

    /** @returns {'i32' | 'bool' | 'str' | 'json'} */
    parseReturnTypeName() {
        const t = this.next();
        if ((t.type === 'kw' || t.type === 'ident') && INTEGER_TYPES.has(t.value)) return 'i32';
        if (t.type === 'kw' && t.value === 'bool') return 'bool';
        if (t.type === 'ident' && t.value === 'String') return 'str';
        if (t.type === 'ident' && t.value === 'Json') return 'json';
        throw new Error(`Tac worker (rust): unsupported return type '${t.value || 'end of input'}' on line ${t.line} (use an i32-compatible integer, bool, String, or Json)`);
    }

    /** Consume a balanced `{ ... }` block (assumes the next token is `{`). */
    skipBracedBlock() {
        this.expectPunct('{');
        let depth = 1;
        while (depth > 0) {
            const t = this.next();
            if (t.type === 'eof')
                throw new Error('Tac worker (rust): unterminated `{ ... }` block');
            if (t.type === 'punct' && t.value === '{') depth++;
            else if (t.type === 'punct' && t.value === '}') depth--;
        }
    }

    /** @returns {{ methods: Node[] }} */
    parseProgram() {
        // Skip leading items the codegen ignores: `use ...;` imports and the
        // `struct`/`enum Handler` type declaration the server handler convention
        // requires (HandlerAdapter keys on it; only the `impl` block is lowered).
        while (this.peek().type === 'ident'
            && (this.peek().value === 'use' || this.peek().value === 'struct' || this.peek().value === 'enum')) {
            this.next(); // 'use' | 'struct' | 'enum'
            while (this.peek().type !== 'eof' && !this.isPunct(';') && !this.isPunct('{')) this.next();
            if (this.isPunct(';')) this.next();
            else if (this.isPunct('{')) this.skipBracedBlock();
        }
        this.expectKw('impl');
        this.expectIdent(); // the impl target name (e.g. Handler)
        this.expectPunct('{');
        /** @type {Node[]} */
        const methods = [];
        while (!this.isPunct('}')) {
            if (this.peek().type === 'eof')
                throw new Error('Tac worker (rust): unexpected end of input inside impl block');
            methods.push(this.parseMethod());
        }
        this.expectPunct('}');
        if (methods.length === 0)
            throw new Error('Tac worker (rust): the impl block must declare at least one `pub fn` method');
        return { methods };
    }

    /** @returns {Node} */
    parseMethod() {
        if (this.isKw('pub')) this.next();
        this.expectKw('fn');
        const name = this.expectIdent();
        this.expectPunct('(');
        // Single parameter: `request: Request` (type ignored). Allow zero params too.
        let paramName = 'request';
        if (!this.isPunct(')')) {
            paramName = this.expectIdent();
            if (this.isPunct(':')) {
                this.next();
                this.expectIdent(); // parameter type name (ignored)
            }
        }
        this.expectPunct(')');
        // Optional primitive return type (defaults to i32).
        let returnType = 'i32';
        if (this.isPunct('->')) {
            this.next();
            returnType = this.parseReturnTypeName();
        }
        const body = this.parseBlock();
        return { kind: 'method', name, paramName, returnType, body };
    }

    /** @returns {Node} A block: { kind:'block', stmts:[...] } */
    parseBlock() {
        this.expectPunct('{');
        /** @type {Node[]} */
        const stmts = [];
        while (!this.isPunct('}')) {
            if (this.peek().type === 'eof')
                throw new Error('Tac worker (rust): unexpected end of input inside a block');
            stmts.push(this.parseStatement());
        }
        this.expectPunct('}');
        return { kind: 'block', stmts };
    }

    /** @returns {Node} */
    parseStatement() {
        if (this.isKw('let')) {
            this.next();
            if (this.isKw('mut')) this.next();
            const name = this.expectIdent();
            this.expectPunct('=');
            const expr = this.parseExpr();
            this.expectPunct(';');
            return { kind: 'let', name, expr };
        }
        if (this.isKw('return')) {
            this.next();
            const expr = this.parseExpr();
            this.expectPunct(';');
            return { kind: 'return', expr };
        }
        if (this.isKw('while')) {
            this.next();
            const cond = this.parseExpr();
            const body = this.parseBlock();
            return { kind: 'while', cond, body };
        }
        // Assignment `name = expr;` (lookahead: ident then '=').
        if (this.peek().type === 'ident' && this.tokens[this.pos + 1]?.value === '=' && this.tokens[this.pos + 1]?.type === 'punct') {
            const name = this.expectIdent();
            this.expectPunct('=');
            const expr = this.parseExpr();
            this.expectPunct(';');
            return { kind: 'assign', name, expr };
        }
        // Expression statement or trailing tail expression.
        const expr = this.parseExpr();
        if (this.isPunct(';')) {
            this.next();
            return { kind: 'exprStmt', expr };
        }
        return { kind: 'tail', expr };
    }

    /** @returns {Node} */
    parseExpr() { return this.parseLogicalOr(); }

    /** @returns {Node} */
    parseLogicalOr() {
        let left = this.parseLogicalAnd();
        while (this.isPunct('||')) {
            const op = this.next().value;
            const right = this.parseLogicalAnd();
            left = { kind: 'binary', op, left, right };
        }
        return left;
    }

    /** @returns {Node} */
    parseLogicalAnd() {
        let left = this.parseComparison();
        while (this.isPunct('&&')) {
            const op = this.next().value;
            const right = this.parseComparison();
            left = { kind: 'binary', op, left, right };
        }
        return left;
    }

    /** @returns {Node} */
    parseComparison() {
        let left = this.parseAdditive();
        while (['==', '!=', '<', '<=', '>', '>='].some((op) => this.isPunct(op))) {
            const op = this.next().value;
            const right = this.parseAdditive();
            left = { kind: 'binary', op, left, right };
        }
        return left;
    }

    /** @returns {Node} */
    parseAdditive() {
        let left = this.parseMultiplicative();
        while (this.isPunct('+') || this.isPunct('-')) {
            const op = this.next().value;
            const right = this.parseMultiplicative();
            left = { kind: 'binary', op, left, right };
        }
        return left;
    }

    /** @returns {Node} */
    parseMultiplicative() {
        let left = this.parseUnary();
        while (this.isPunct('*') || this.isPunct('/') || this.isPunct('%')) {
            const op = this.next().value;
            const right = this.parseUnary();
            left = { kind: 'binary', op, left, right };
        }
        return left;
    }

    /** @returns {Node} */
    parseUnary() {
        if (this.isPunct('-')) {
            this.next();
            return { kind: 'unary', op: '-', expr: this.parseUnary() };
        }
        if (this.isPunct('!')) {
            this.next();
            return { kind: 'unary', op: '!', expr: this.parseUnary() };
        }
        return this.parsePrimary();
    }

    /** @returns {Node} */
    parsePrimary() {
        const t = this.peek();
        if (t.type === 'int') { this.next(); return { kind: 'int', value: Number(t.value) }; }
        if (t.type === 'kw' && (t.value === 'true' || t.value === 'false')) {
            this.next();
            return { kind: 'bool', value: t.value === 'true' };
        }
        if (t.type === 'str') { this.next(); return { kind: 'strlit', value: t.value }; }
        if (this.isPunct('(')) {
            this.next();
            const expr = this.parseExpr();
            this.expectPunct(')');
            return expr;
        }
        if (this.isKw('if')) {
            this.next();
            const cond = this.parseExpr();
            const then = this.parseBlock();
            this.expectKw('else');
            const otherwise = this.parseBlock();
            return { kind: 'if', cond, then, otherwise };
        }
        if (t.type === 'ident') {
            const name = this.next().value;
            if (name === 'json' && this.isPunct('(')) {
                this.next();
                const expr = this.parseExpr();
                this.expectPunct(')');
                return { kind: 'jsonRaw', expr };
            }
            // Method/builtin call: `name.member(...)`.
            if (this.isPunct('.')) {
                this.next();
                const member = this.expectIdent();
                this.expectPunct('(');
                this.expectPunct(')');
                if (member === 'len') return { kind: 'requestLen', receiver: name };
                if (member === 'body') return { kind: 'requestBody', receiver: name };
                if (member === 'json') return { kind: 'requestJson', receiver: name };
                throw new Error(`Tac worker (rust): unsupported call '.${member}()' on line ${t.line} (request.len(), request.body(), and request.json() are available)`);
            }
            return { kind: 'var', name };
        }
        throw new Error(`Tac worker (rust): unexpected token '${t.value || 'end of input'}' on line ${t.line}`);
    }
}

/**
 * Compile Rust-subset worker source into worker-ABI wasm bytes. Parsing yields
 * the shared handler AST; `compileHandlerProgram` does the rest.
 * @param {string} source
 * @returns {Uint8Array}
 */
export function compileRustWorker(source) {
    return compileHandlerProgram(new Parser(tokenize(source)).parseProgram());
}
