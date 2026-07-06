// @ts-check
//
// Shared Tac handler subset -> Wasm frontend.
//
// Parses a normalized worker source - `class Handler { public: static
// int|bool|string|json VERB(Request request) { ... } };` - into the shared handler AST.
// Type checking, Wasm codegen, and the verb dispatcher live in
// `tac-handler-codegen.js`. No clang/LLVM is involved.
//
// Internal Tac syntax: integer aliases, `bool`,
// `string`, and `json`
// types, `let`-style declarations, assignment, arithmetic (`+ - * / %`),
// comparisons, the ternary operator `?:` for conditionals (the subset has no
// if-expression), `while` loops, string literals + concatenation (`+`, with
// i32 auto-converted), `request.len()` / `request.body()` / `request.json()` /
// `request.platform("key")`,
// `json(stringExpr)`, and a final `return` per method.

import { compileHandlerProgram } from './tac-handler-codegen.js';

/** @typedef {Record<string, any>} Node */
/** @typedef {{ type: 'ident' | 'int' | 'str' | 'kw' | 'punct' | 'eof', value: string, line: number }} Token */

const KEYWORDS = new Set([
    'class', 'public', 'private', 'protected', 'static', 'const',
    'int', 'char', 'short', 'long', 'signed', 'unsigned', 'bool', 'true', 'false',
    'byte', 'sbyte', 'ushort', 'uint',
    'string', 'json', 'while', 'if', 'else', 'return',
]);
const I32_TYPES = new Set([
    'int', 'char', 'short', 'long', 'signed', 'unsigned',
    'byte', 'sbyte', 'ushort', 'uint',
    'i8', 'i16', 'i32', 'u8', 'u16', 'u32', 'isize', 'usize',
    'int8', 'uint8', 'int16', 'uint16', 'int32', 'uint32', 'uintptr',
    'int8_t', 'uint8_t', 'int16_t', 'uint16_t', 'int32_t', 'uint32_t',
]);
const TYPE_STARTS = new Set([...I32_TYPES, 'bool', 'string', 'json']);
const PUNCT2 = ['==', '!=', '<=', '>=', '&&', '||', '::'];
const PUNCT1 = ['{', '}', '(', ')', '[', ']', ';', ',', ':', '.', '=', '<', '>', '+', '-', '*', '/', '%', '&', '?', '!'];

/**
 * Tokenize normalized Tac worker source.
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
        // Preprocessor directive (`#include`, `#pragma`, etc.): skip the whole line.
        if (c === '#') {
            while (i < source.length && source[i] !== '\n') i += 1;
            continue;
        }
        if (c === '/' && source[i + 1] === '/') {
            while (i < source.length && source[i] !== '\n') i += 1;
            continue;
        }
        if (c === '/' && source[i + 1] === '*') {
            i += 2;
            while (i < source.length && !(source[i] === '*' && source[i + 1] === '/')) {
                if (source[i] === '\n') line += 1;
                i += 1;
            }
            i += 2;
            continue;
        }
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
                throw new Error(`Tac worker subset: unterminated string literal on line ${line}`);
            i += 1;
            tokens.push({ type: 'str', value, line });
            continue;
        }
        if (isIdentStart(c)) {
            const start = i;
            while (i < source.length && isIdent(source[i])) i += 1;
            const value = source.slice(start, i);
            tokens.push({ type: KEYWORDS.has(value) ? 'kw' : 'ident', value, line });
            continue;
        }
        if (isDigit(c)) {
            const start = i;
            while (i < source.length && (isDigit(source[i]) || source[i] === '_')) i += 1;
            tokens.push({ type: 'int', value: source.slice(start, i).replace(/_/g, ''), line });
            continue;
        }
        const two = source.slice(i, i + 2);
        if (PUNCT2.includes(two)) { tokens.push({ type: 'punct', value: two, line }); i += 2; continue; }
        if (PUNCT1.includes(c)) { tokens.push({ type: 'punct', value: c, line }); i += 1; continue; }
        throw new Error(`Tac worker subset: unexpected character '${c}' on line ${line}`);
    }
    tokens.push({ type: 'eof', value: '', line });
    return tokens;
}

/** Recursive-descent parser for the normalized Tac handler subset. */
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
    isTypeStart() {
        const t = this.peek();
        return (t.type === 'kw' || t.type === 'ident') && TYPE_STARTS.has(t.value);
    }

    /** @param {string} value */
    expectPunct(value) {
        const t = this.next();
        if (t.type !== 'punct' || t.value !== value)
            throw new Error(`Tac worker subset: expected '${value}' but found '${t.value || 'end of input'}' on line ${t.line}`);
    }

    /** @param {string} value */
    expectKw(value) {
        const t = this.next();
        if (t.type !== 'kw' || t.value !== value)
            throw new Error(`Tac worker subset: expected '${value}' but found '${t.value || 'end of input'}' on line ${t.line}`);
    }

    /** @returns {string} */
    expectIdent() {
        const t = this.next();
        if (t.type !== 'ident')
            throw new Error(`Tac worker subset: expected an identifier but found '${t.value || 'end of input'}' on line ${t.line}`);
        return t.value;
    }

    /** @returns {{ methods: Node[] }} */
    parseProgram() {
        // Skip leading `using ...;` declarations.
        while (this.peek().type === 'ident' && this.peek().value === 'using') {
            while (this.peek().type !== 'eof' && !this.isPunct(';')) this.next();
            if (this.isPunct(';')) this.next();
        }
        this.expectKw('class');
        this.expectIdent(); // class name (e.g. Handler)
        this.expectPunct('{');
        /** @type {Node[]} */
        const methods = [];
        while (!this.isPunct('}')) {
            if (this.peek().type === 'eof')
                throw new Error('Tac worker subset: unexpected end of input inside the class body');
            // Access specifier: `public:` / `private:` / `protected:`.
            if (this.isKw('public') || this.isKw('private') || this.isKw('protected')) {
                this.next();
                this.expectPunct(':');
                continue;
            }
            methods.push(this.parseMethod());
        }
        this.expectPunct('}');
        if (this.isPunct(';')) this.next(); // trailing class-declaration semicolon
        return { methods };
    }

    /** @returns {'i32' | 'bool' | 'str' | 'json'} */
    parseValueType() {
        const t = this.next();
        if ((t.type === 'kw' || t.type === 'ident') && (t.value === 'signed' || t.value === 'unsigned')) {
            const base = this.next();
            if ((base.type === 'kw' || base.type === 'ident') && I32_TYPES.has(base.value)) return 'i32';
            throw new Error(`Tac worker subset: expected an integer type after '${t.value}' on line ${t.line}`);
        }
        if ((t.type === 'kw' || t.type === 'ident') && I32_TYPES.has(t.value)) return 'i32';
        if (t.type === 'kw' && t.value === 'bool') return 'bool';
        if (t.type === 'kw' && t.value === 'string') return 'str';
        if (t.type === 'kw' && t.value === 'json') return 'json';
        throw new Error(`Tac worker subset: expected a type (integer, bool, string, or json) but found '${t.value || 'end of input'}' on line ${t.line}`);
    }

    /** @returns {Node} */
    parseMethod() {
        this.expectKw('static');
        const returnType = this.parseValueType();
        const name = this.expectIdent();
        this.expectPunct('(');
        let paramName = 'request';
        if (!this.isPunct(')')) {
            if (this.isKw('const')) this.next();
            this.expectIdent(); // parameter type (e.g. Request)
            if (this.isPunct('&')) this.next();
            paramName = this.expectIdent();
        }
        this.expectPunct(')');
        const body = this.parseBlock();
        return { kind: 'method', name, paramName, returnType, body };
    }

    /** @returns {Node} */
    parseBlock() {
        this.expectPunct('{');
        /** @type {Node[]} */
        const stmts = [];
        while (!this.isPunct('}')) {
            if (this.peek().type === 'eof')
                throw new Error('Tac worker subset: unexpected end of input inside a block');
            stmts.push(this.parseStatement());
        }
        this.expectPunct('}');
        return { kind: 'block', stmts };
    }

    /** @returns {Node} */
    parseStatement() {
        // Local declaration: the declared type is parsed, then inferred from the initializer.
        if (this.isTypeStart()) {
            this.parseValueType();
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
            this.expectPunct('(');
            const cond = this.parseExpr();
            this.expectPunct(')');
            const body = this.parseBlock();
            return { kind: 'while', cond, body };
        }
        if (this.isKw('if')) {
            throw new Error(`Tac worker subset: 'if' statements are not supported on line ${this.peek().line}; use the ternary operator '?:' for conditionals`);
        }
        // Assignment: `name = expr;`
        if (this.peek().type === 'ident' && this.tokens[this.pos + 1]?.type === 'punct' && this.tokens[this.pos + 1]?.value === '=') {
            const name = this.expectIdent();
            this.expectPunct('=');
            const expr = this.parseExpr();
            this.expectPunct(';');
            return { kind: 'assign', name, expr };
        }
        const expr = this.parseExpr();
        this.expectPunct(';');
        return { kind: 'exprStmt', expr };
    }

    /** @param {Node} expr @returns {Node} A block whose value is `expr`. */
    static valueBlock(expr) {
        return { kind: 'block', stmts: [{ kind: 'tail', expr }] };
    }

    /** @returns {Node} */
    parseExpr() { return this.parseTernary(); }

    /** @returns {Node} */
    parseTernary() {
        const cond = this.parseLogicalOr();
        if (this.isPunct('?')) {
            this.next();
            const thenExpr = this.parseExpr();
            this.expectPunct(':');
            const elseExpr = this.parseExpr();
            return { kind: 'if', cond, then: Parser.valueBlock(thenExpr), otherwise: Parser.valueBlock(elseExpr) };
        }
        return cond;
    }

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
        // JSON object literal in value position: `{ "key": expr, ... }`. Method
        // bodies and loops are parsed elsewhere, so a `{` in an expression is
        // unambiguously an object literal.
        if (this.isPunct('{')) {
            return this.parseObjectLiteral();
        }
        if (t.type === 'ident' || (t.type === 'kw' && t.value === 'json')) {
            const name = this.next().value;
            if (name === 'json' && this.isPunct('(')) {
                this.next();
                const expr = this.parseExpr();
                this.expectPunct(')');
                return { kind: 'jsonRaw', expr };
            }
            // Member call: `name.len()` (only request.len() is supported).
            if (this.isPunct('.')) {
                this.next();
                const memberToken = this.next();
                if (memberToken.type !== 'ident' && !(memberToken.type === 'kw' && memberToken.value === 'json'))
                    throw new Error(`Tac worker subset: expected a member name but found '${memberToken.value || 'end of input'}' on line ${memberToken.line}`);
                const member = memberToken.value;
                this.expectPunct('(');
                // Keyed request accessors take a string literal: request.query("k").
                if (member === 'query' || member === 'path' || member === 'header' || member === 'platform') {
                    const keyToken = this.next();
                    if (keyToken.type !== 'str')
                        throw new Error(`Tac worker subset: request.${member}(...) requires a string key on line ${t.line}`);
                    this.expectPunct(')');
                    const kind = member === 'query' ? 'requestQuery' : member === 'path' ? 'requestPath' : member === 'header' ? 'requestHeader' : 'requestPlatform';
                    return { kind, key: keyToken.value, receiver: name };
                }
                this.expectPunct(')');
                if (member === 'len') return { kind: 'requestLen', receiver: name };
                if (member === 'body') return { kind: 'requestBody', receiver: name };
                if (member === 'json') return { kind: 'requestJson', receiver: name };
                throw new Error(`Tac worker subset: unsupported call '.${member}()' on line ${t.line} (request.len(), request.body(), request.json(), request.query("k"), request.path("k"), request.header("k"), request.platform("key") are available)`);
            }
            return { kind: 'var', name };
        }
        throw new Error(`Tac worker subset: unexpected token '${t.value || 'end of input'}' on line ${t.line}`);
    }

    /** @returns {Node} A `{ "key": expr, ... }` JSON object literal. */
    parseObjectLiteral() {
        this.expectPunct('{');
        /** @type {Node[]} */
        const entries = [];
        while (!this.isPunct('}')) {
            const keyToken = this.next();
            if (keyToken.type !== 'str')
                throw new Error(`Tac worker subset: object keys must be string literals on line ${keyToken.line}`);
            this.expectPunct(':');
            entries.push({ key: keyToken.value, value: this.parseExpr() });
            if (this.isPunct(',')) this.next();
            else break;
        }
        this.expectPunct('}');
        return { kind: 'jsonObject', entries };
    }
}

/**
 * Compile normalized Tac worker source into worker-ABI wasm bytes.
 * @param {string} source
 * @returns {Uint8Array}
 */
export function compileTacSubsetWorker(source) {
    return compileHandlerProgram(new Parser(tokenize(source)).parseProgram());
}
