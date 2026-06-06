// @ts-check
//
// In-house Python (handler subset) -> Wasm frontend.
//
// Parses a `yon`-shaped worker source - `class Handler` with methods named
// after HTTP verbs - into the shared handler AST. Type checking, Wasm codegen,
// and the verb dispatcher live in `tac-handler-codegen.js`. No CPython,
// Pyodide, Emscripten, or external compiler is involved.
//
// Tac-Python dialect (a documented subset): `int`/`bool`/`str`/`json` return
// types, assignments, arithmetic, comparisons, logical operators, Python ternary expressions
// (`a if condition else b`), `while` loops, string literals + concatenation,
// `request.len()` / `request.body()` / `request.json()`, `json(string_expr)`,
// and a `return` per method.

import { compileHandlerProgram } from './tac-handler-codegen.js';

/** @typedef {Record<string, any>} Node */
/** @typedef {{ indent: number, text: string, line: number }} SourceLine */
/** @typedef {{ type: 'ident' | 'int' | 'str' | 'kw' | 'punct' | 'eof', value: string, line: number }} Token */

const KEYWORDS = new Set(['if', 'else', 'while', 'return', 'class', 'def', 'int', 'bool', 'str', 'staticmethod', 'True', 'False', 'and', 'or', 'not']);
const PUNCT2 = ['==', '!=', '<=', '>='];
const PUNCT1 = ['(', ')', '[', ']', ':', '.', '=', '<', '>', '+', '-', '*', '/', '%', ','];

/**
 * Convert indentation-sensitive Python source into significant lines.
 * @param {string} source
 * @returns {SourceLine[]}
 */
export function significantLines(source) {
    /** @type {SourceLine[]} */
    const lines = [];
    const rawLines = source.replace(/\r\n?/g, '\n').split('\n');
    for (let index = 0; index < rawLines.length; index += 1) {
        const raw = rawLines[index];
        if (/^\s*$/.test(raw)) continue;
        const indentMatch = raw.match(/^[ \t]*/)?.[0] ?? '';
        if (indentMatch.includes('\t'))
            throw new Error(`Tac worker (python): tabs are not supported for indentation on line ${index + 1}`);
        const withoutComment = stripLineComment(raw.slice(indentMatch.length)).trimEnd();
        if (withoutComment.trim().length === 0) continue;
        lines.push({ indent: indentMatch.length, text: withoutComment.trim(), line: index + 1 });
    }
    return lines;
}

/**
 * Remove comments outside string literals.
 * @param {string} text
 * @returns {string}
 */
function stripLineComment(text) {
    let quote = '';
    for (let i = 0; i < text.length; i += 1) {
        const c = text[i];
        if (quote) {
            if (c === '\\') { i += 1; continue; }
            if (c === quote) quote = '';
            continue;
        }
        if (c === '"' || c === "'") { quote = c; continue; }
        if (c === '#') return text.slice(0, i);
    }
    return text;
}

/**
 * Tokenize a Tac-Python expression.
 * @param {string} source
 * @param {number} line
 * @returns {Token[]}
 */
export function tokenizeExpr(source, line = 1) {
    /** @type {Token[]} */
    const tokens = [];
    let i = 0;
    const isIdentStart = (/** @type {string} */ c) => /[A-Za-z_]/.test(c);
    const isIdent = (/** @type {string} */ c) => /[A-Za-z0-9_]/.test(c);
    const isDigit = (/** @type {string} */ c) => c >= '0' && c <= '9';

    while (i < source.length) {
        const c = source[i];
        if (c === ' ' || c === '\t') { i += 1; continue; }
        if (c === '"' || c === "'") {
            const quote = c;
            i += 1;
            let value = '';
            while (i < source.length && source[i] !== quote) {
                if (source[i] === '\\') {
                    const next = source[i + 1];
                    value += next === 'n' ? '\n' : next === 't' ? '\t' : next === 'r' ? '\r' : next ?? '';
                    i += 2;
                }
                else {
                    value += source[i];
                    i += 1;
                }
            }
            if (source[i] !== quote)
                throw new Error(`Tac worker (python): unterminated string literal on line ${line}`);
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
        throw new Error(`Tac worker (python): unexpected character '${c}' on line ${line}`);
    }
    tokens.push({ type: 'eof', value: '', line });
    return tokens;
}

/** Parser for the indentation-sensitive Tac-Python handler subset. */
export class Parser {
    /** @param {SourceLine[]} lines */
    constructor(lines) {
        this.lines = lines;
        this.pos = 0;
        /** @type {Set<string>} */
        this.bindings = new Set();
    }

    /** @returns {SourceLine | undefined} */
    peekLine() { return this.lines[this.pos]; }
    /** @returns {SourceLine} */
    nextLine() { return /** @type {SourceLine} */ (this.lines[this.pos++]); }

    /** @returns {{ methods: Node[] }} */
    parseProgram() {
        while (this.peekLine() && /^(?:from|import)\s+/.test(/** @type {SourceLine} */ (this.peekLine()).text))
            this.nextLine();
        const head = this.nextLine();
        const classMatch = head.text.match(/^class\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*$/);
        if (!classMatch)
            throw new Error(`Tac worker (python): expected 'class Handler:' on line ${head.line}`);
        /** @type {Node[]} */
        const methods = [];
        while (this.peekLine()) {
            const line = /** @type {SourceLine} */ (this.peekLine());
            if (line.indent <= head.indent) break;
            if (line.text.startsWith('@')) { this.nextLine(); continue; }
            methods.push(this.parseMethod(line.indent));
        }
        return { methods };
    }

    /** @param {number} indent @returns {Node} */
    parseMethod(indent) {
        const line = this.nextLine();
        if (line.indent !== indent)
            throw new Error(`Tac worker (python): inconsistent method indentation on line ${line.line}`);
        const match = line.text.match(/^def\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)\s*->\s*(int|bool|str|json)\s*:\s*$/);
        if (!match)
            throw new Error(`Tac worker (python): expected 'def VERB(request) -> int|bool|str|json:' on line ${line.line}`);
        const [, name, params, declaredType] = match;
        const paramNames = params.split(',').map((part) => part.trim()).filter(Boolean).map((part) => {
            const withoutDefault = part.split('=')[0]?.trim() ?? '';
            return withoutDefault.split(':')[0]?.trim() ?? '';
        }).filter((part) => part && part !== 'self' && part !== 'cls');
        const paramName = paramNames[0] ?? 'request';
        this.bindings = new Set();
        const body = this.parseBlock(indent + 1);
        return {
            kind: 'method',
            name,
            paramName,
            returnType: declaredType === 'str' ? 'str' : declaredType === 'json' ? 'json' : declaredType === 'bool' ? 'bool' : 'i32',
            body,
        };
    }

    /** @param {number} parentIndent @returns {Node} */
    parseBlock(parentIndent) {
        const first = this.peekLine();
        if (!first || first.indent < parentIndent)
            throw new Error('Tac worker (python): expected an indented block');
        const blockIndent = first.indent;
        if (blockIndent < parentIndent)
            throw new Error(`Tac worker (python): expected an indented block on line ${first.line}`);
        /** @type {Node[]} */
        const stmts = [];
        while (this.peekLine()) {
            const line = /** @type {SourceLine} */ (this.peekLine());
            if (line.indent < blockIndent) break;
            if (line.indent > blockIndent)
                throw new Error(`Tac worker (python): unexpected indentation on line ${line.line}`);
            stmts.push(this.parseStatement(blockIndent));
        }
        return { kind: 'block', stmts };
    }

    /** @param {number} indent @returns {Node} */
    parseStatement(indent) {
        const line = this.nextLine();
        if (line.text.startsWith('while ')) {
            const condSource = line.text.match(/^while\s+([\s\S]+):\s*$/)?.[1];
            if (!condSource)
                throw new Error(`Tac worker (python): expected 'while condition:' on line ${line.line}`);
            return {
                kind: 'while',
                cond: this.parseExpr(condSource, line.line),
                body: this.parseBlock(indent + 1),
            };
        }
        if (line.text.startsWith('return ')) {
            return { kind: 'return', expr: this.parseExpr(line.text.slice('return '.length), line.line) };
        }
        const assignment = line.text.match(/^([A-Za-z_][A-Za-z0-9_]*)(?:\s*:\s*(?:int|bool|str|json))?\s*=\s*([\s\S]+)$/);
        if (assignment) {
            const [, name, exprSource] = assignment;
            const kind = this.bindings.has(name) ? 'assign' : 'let';
            this.bindings.add(name);
            return { kind, name, expr: this.parseExpr(exprSource, line.line) };
        }
        return { kind: 'exprStmt', expr: this.parseExpr(line.text, line.line) };
    }

    /** @param {string} source @param {number} line @returns {Node} */
    parseExpr(source, line) {
        return new ExprParser(tokenizeExpr(source, line)).parseExpr();
    }
}

/** Recursive-descent expression parser for Tac-Python expressions. */
class ExprParser {
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
            throw new Error(`Tac worker (python): expected '${value}' but found '${t.value || 'end of input'}' on line ${t.line}`);
    }

    /** @returns {string} */
    expectIdent() {
        const t = this.next();
        if (t.type !== 'ident')
            throw new Error(`Tac worker (python): expected an identifier but found '${t.value || 'end of input'}' on line ${t.line}`);
        return t.value;
    }

    /** @returns {Node} */
    parseExpr() {
        const expr = this.parsePythonTernary();
        if (this.peek().type !== 'eof')
            throw new Error(`Tac worker (python): unexpected token '${this.peek().value}' on line ${this.peek().line}`);
        return expr;
    }

    /** @returns {Node} */
    parsePythonTernary() {
        const thenExpr = this.parseLogicalOr();
        if (this.isKw('if')) {
            this.next();
            const cond = this.parseLogicalOr();
            if (!this.isKw('else'))
                throw new Error(`Tac worker (python): expected 'else' in ternary expression on line ${this.peek().line}`);
            this.next();
            const elseExpr = this.parsePythonTernary();
            return {
                kind: 'if',
                cond,
                then: ExprParser.valueBlock(thenExpr),
                otherwise: ExprParser.valueBlock(elseExpr),
            };
        }
        return thenExpr;
    }

    /** @returns {Node} */
    parseLogicalOr() {
        let left = this.parseLogicalAnd();
        while (this.isKw('or')) {
            this.next();
            const right = this.parseLogicalAnd();
            left = { kind: 'binary', op: '||', left, right };
        }
        return left;
    }

    /** @returns {Node} */
    parseLogicalAnd() {
        let left = this.parseComparison();
        while (this.isKw('and')) {
            this.next();
            const right = this.parseComparison();
            left = { kind: 'binary', op: '&&', left, right };
        }
        return left;
    }

    /** @param {Node} expr @returns {Node} */
    static valueBlock(expr) {
        return { kind: 'block', stmts: [{ kind: 'tail', expr }] };
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
        if (this.isKw('not')) {
            this.next();
            return { kind: 'unary', op: '!', expr: this.parseUnary() };
        }
        return this.parsePrimary();
    }

    /** @returns {Node} */
    parsePrimary() {
        const t = this.peek();
        if (t.type === 'int') { this.next(); return { kind: 'int', value: Number(t.value) }; }
        if (t.type === 'kw' && (t.value === 'True' || t.value === 'False')) {
            this.next();
            return { kind: 'bool', value: t.value === 'True' };
        }
        if (t.type === 'str') { this.next(); return { kind: 'strlit', value: t.value }; }
        if (this.isPunct('(')) {
            this.next();
            const expr = this.parsePythonTernary();
            this.expectPunct(')');
            return expr;
        }
        if (t.type === 'ident') {
            const name = this.next().value;
            if (name === 'json' && this.isPunct('(')) {
                this.next();
                const expr = this.parsePythonTernary();
                this.expectPunct(')');
                return { kind: 'jsonRaw', expr };
            }
            if (this.isPunct('.')) {
                this.next();
                const member = this.expectIdent();
                this.expectPunct('(');
                this.expectPunct(')');
                if (member === 'len') return { kind: 'requestLen', receiver: name };
                if (member === 'body') return { kind: 'requestBody', receiver: name };
                if (member === 'json') return { kind: 'requestJson', receiver: name };
                throw new Error(`Tac worker (python): unsupported call '.${member}()' on line ${t.line} (request.len(), request.body(), and request.json() are available)`);
            }
            return { kind: 'var', name };
        }
        throw new Error(`Tac worker (python): unexpected token '${t.value || 'end of input'}' on line ${t.line}`);
    }
}

/**
 * Compile Tac-Python worker source into worker-ABI wasm bytes.
 * @param {string} source
 * @returns {Uint8Array}
 */
export function compilePythonWorker(source) {
    return compileHandlerProgram(new Parser(significantLines(source)).parseProgram());
}
