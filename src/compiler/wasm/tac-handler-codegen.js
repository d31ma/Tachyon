// @ts-check
//
// Language-agnostic Tac Worker codegen.
//
// Every in-house frontend (Rust, C++, etc.) is just a parser that produces this
// shared handler AST; this module type-checks it, lowers each method to a Wasm
// function, and builds the verb-dispatching `call` with JSON responses. The
// AST contract:
//
//   program  = { methods: Method[] }
//   Method   = { kind:'method', name, returnType:'i32'|'bool'|'str'|'json', body: Block }
//   Block    = { kind:'block', stmts: Stmt[] }
//   Stmt     = let{name,expr} | assign{name,expr} | while{cond,body}
//            | return{expr} | tail{expr} | exprStmt{expr}
//   Expr     = int{value} | bool{value} | strlit{value} | var{name} | requestLen
//            | requestJson | jsonRaw{expr}
//            | unary{op:'-'|'!',expr} | binary{op,left,right} | if{cond,then,otherwise}
//
// `name` on a method must be an HTTP request verb; string/json values are
// runtime pointers to a `{ dataPtr@0, byteLen@4 }` header (see
// tac-wasm-compiler.js). JSON values are raw JSON byte ranges, not escaped
// strings.

import { Emitter, VAL } from './wasm-module.js';
import { buildWorkerModule } from './tac-wasm-compiler.js';

/** @typedef {Record<string, any>} Node */
/** @typedef {'i32' | 'bool' | 'str' | 'json'} ValueType */
/** @typedef {Map<string, { index: number, type: ValueType }>} LocalScope */

/** Handler methods are HTTP request methods, dispatched by the request verb. */
export const HTTP_METHODS = new Set(['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']);

const BINARY_OPS = {
    '+': /** @param {Emitter} e */ (e) => e.i32Add(),
    '-': /** @param {Emitter} e */ (e) => e.i32Sub(),
    '*': /** @param {Emitter} e */ (e) => e.i32Mul(),
    // '/' and '%' are emitted via the trap-safe idiv/imod runtime helpers.
    '==': /** @param {Emitter} e */ (e) => e.i32Eq(),
    '!=': /** @param {Emitter} e */ (e) => e.i32Ne(),
    '<': /** @param {Emitter} e */ (e) => e.i32LtS(),
    '<=': /** @param {Emitter} e */ (e) => e.i32LeS(),
    '>': /** @param {Emitter} e */ (e) => e.i32GtS(),
    '>=': /** @param {Emitter} e */ (e) => e.i32GeS(),
};

const ARITHMETIC_OPS = new Set(['+', '-', '*', '/', '%']);
const COMPARISON_OPS = new Set(['==', '!=', '<', '<=', '>', '>=']);
const LOGICAL_OPS = new Set(['&&', '||']);

/** @param {ValueType} type */
const isConditionType = (type) => type === 'i32' || type === 'bool';
/** @param {ValueType} type */
const label = (type) => (type === 'str' ? 'String' : type === 'json' ? 'Json' : type === 'bool' ? 'bool' : 'i32');

/**
 * Type-check a method and assign Wasm local indices. Infers each expression's
 * type (`i32`, `bool`, `str`, or `json`), records `let` bindings (flat scope, no shadowing),
 * annotates expression nodes with `.type`, validates the declared return type,
 * and confirms the method name is an HTTP request verb.
 * @param {Node} method
 * @returns {{ scope: LocalScope, valueType: ValueType }}
 */
export function analyzeMethod(method) {
    if (!HTTP_METHODS.has(method.name))
        throw new Error(`Tac worker: handler method '${method.name}' must be an HTTP request method (one of ${[...HTTP_METHODS].join(', ')})`);

    /** @type {LocalScope} */
    const scope = new Map();
    let nextIndex = 2; // 0 = requestPtr, 1 = requestLen

    /** @param {Node} node @returns {ValueType} */
    const inferExpr = (node) => {
        /** @type {ValueType} */
        let type;
        switch (node.kind) {
            case 'int': type = 'i32'; break;
            case 'bool': type = 'bool'; break;
            case 'strlit': type = 'str'; break;
            case 'requestLen': type = 'i32'; break;
            case 'requestBody': type = 'str'; break;
            case 'requestJson': type = 'json'; break;
            case 'requestQuery':
            case 'requestPath':
            case 'requestHeader': type = 'str'; break;
            case 'jsonRaw': {
                const innerType = inferExpr(node.expr);
                if (innerType !== 'str' && innerType !== 'json')
                    throw new Error(`Tac worker: json(...) requires a String or Json operand`);
                type = 'json';
                break;
            }
            case 'var': {
                const local = scope.get(node.name);
                if (!local) throw new Error(`Tac worker: unknown variable '${node.name}'`);
                type = local.type;
                break;
            }
            case 'unary':
                if (node.op === '!') {
                    if (!isConditionType(inferExpr(node.expr)))
                        throw new Error(`Tac worker: unary '!' requires a bool or i32 operand`);
                    type = 'bool';
                    break;
                }
                if (inferExpr(node.expr) !== 'i32') throw new Error(`Tac worker: unary '-' requires an i32 operand`);
                type = 'i32';
                break;
            case 'binary': {
                const lt = inferExpr(node.left);
                const rt = inferExpr(node.right);
                if (lt === 'json' || rt === 'json')
                    throw new Error(`Tac worker: operator '${node.op}' cannot be applied to Json values`);
                if (node.op === '+' && (lt === 'str' || rt === 'str')) {
                    if ((lt !== 'str' && lt !== 'i32') || (rt !== 'str' && rt !== 'i32'))
                        throw new Error(`Tac worker: string concatenation supports String and i32 operands`);
                    type = 'str'; // string concatenation
                }
                else if (LOGICAL_OPS.has(node.op)) {
                    if (!isConditionType(lt) || !isConditionType(rt))
                        throw new Error(`Tac worker: operator '${node.op}' requires bool or i32 operands`);
                    type = 'bool';
                }
                else if (COMPARISON_OPS.has(node.op)) {
                    if ((node.op === '==' || node.op === '!=') && lt === rt && (lt === 'i32' || lt === 'bool')) {
                        type = 'bool';
                    }
                    else {
                        if (lt !== 'i32' || rt !== 'i32')
                            throw new Error(`Tac worker: operator '${node.op}' requires i32 operands`);
                        type = 'bool';
                    }
                }
                else if (ARITHMETIC_OPS.has(node.op)) {
                    if (lt !== 'i32' || rt !== 'i32')
                        throw new Error(`Tac worker: operator '${node.op}' requires i32 operands`);
                    type = 'i32';
                }
                else {
                    throw new Error(`Tac worker: unsupported operator '${node.op}'`);
                }
                break;
            }
            case 'if': {
                if (!isConditionType(inferExpr(node.cond))) throw new Error(`Tac worker: condition must be a bool or i32`);
                const thenType = inferBlock(node.then);
                const elseType = inferBlock(node.otherwise);
                if (thenType !== elseType)
                    throw new Error(`Tac worker: both branches of a conditional must have the same type ('${thenType}' vs '${elseType}')`);
                type = thenType;
                break;
            }
            default:
                throw new Error(`Tac worker: expression kind '${node.kind}' cannot produce a value`);
        }
        node.type = type;
        return type;
    };

    /** @param {Node} stmt */
    const inferVoidStmt = (stmt) => {
        switch (stmt.kind) {
            case 'let': {
                if (scope.has(stmt.name))
                    throw new Error(`Tac worker: duplicate binding '${stmt.name}' (shadowing is not supported)`);
                const type = inferExpr(stmt.expr);
                scope.set(stmt.name, { index: nextIndex++, type });
                break;
            }
            case 'assign': {
                const local = scope.get(stmt.name);
                if (!local) throw new Error(`Tac worker: assignment to unknown variable '${stmt.name}'`);
                if (inferExpr(stmt.expr) !== local.type)
                    throw new Error(`Tac worker: cannot change the type of '${stmt.name}' (declared '${local.type}')`);
                break;
            }
            case 'while':
                if (!isConditionType(inferExpr(stmt.cond))) throw new Error(`Tac worker: loop condition must be a bool or i32`);
                for (const inner of stmt.body.stmts) inferVoidStmt(inner);
                break;
            case 'exprStmt':
            case 'tail':
                inferExpr(stmt.expr);
                break;
            default:
                throw new Error(`Tac worker: '${stmt.kind}' is not valid here`);
        }
    };

    /** @param {Node} block @returns {ValueType} */
    const inferBlock = (block) => {
        const stmts = block.stmts;
        if (stmts.length === 0) throw new Error('Tac worker: a block in value position must end with an expression');
        for (let i = 0; i < stmts.length - 1; i++) {
            if (stmts[i].kind === 'tail') throw new Error('Tac worker: a trailing expression must be the last statement of a block');
            inferVoidStmt(stmts[i]);
        }
        const last = stmts[stmts.length - 1];
        if (last.kind === 'tail' || last.kind === 'return') return inferExpr(last.expr);
        throw new Error('Tac worker: a method/block must end with an expression value');
    };

    const valueType = inferBlock(method.body);
    if (method.returnType !== valueType) {
        throw new Error(`Tac worker: method '${method.name}' is declared to return ${label(method.returnType)} but its body produces ${label(valueType)}`);
    }
    return { scope, valueType };
}

/**
 * Emit an expression, leaving its value on the stack (an i32, or - for string
 * expressions - an i32 pointer to a `{ dataPtr, byteLen }` header).
 * @param {Emitter} e
 * @param {Node} node
 * @param {LocalScope} scope
 * @param {import('./tac-wasm-compiler.js').WorkerRuntimeContext} ctx
 */
function emitExpr(e, node, scope, ctx) {
    switch (node.kind) {
        case 'int':
            e.i32Const(node.value | 0);
            return;
        case 'bool':
            e.i32Const(node.value ? 1 : 0);
            return;
        case 'strlit': {
            const segment = ctx.intern(node.value);
            e.i32Const(segment.offset).i32Const(segment.length).call(ctx.fns.mkStr);
            return;
        }
        case 'requestLen':
            e.localGet(1);
            return;
        case 'requestBody':
            // Wrap the dispatcher-provided body bytes as a string value.
            e.globalGet(ctx.globals.bodyPtr).globalGet(ctx.globals.bodyLen).call(ctx.fns.mkStr);
            return;
        case 'requestJson':
            // Wrap the full request-envelope JSON bytes as a raw JSON value.
            e.localGet(0).localGet(1).call(ctx.fns.mkStr);
            return;
        case 'requestQuery':
        case 'requestPath':
        case 'requestHeader': {
            // Look the field up through a host-provided import. The key is a
            // compile-time constant; the host returns a string-header pointer.
            if (!ctx.imports)
                throw new Error('Tac worker: request.query/path/header requires host imports (internal: not declared)');
            const importIndex = node.kind === 'requestQuery' ? ctx.imports.query
                : node.kind === 'requestPath' ? ctx.imports.path
                    : ctx.imports.header;
            const segment = ctx.intern(String(node.key ?? ''));
            e.i32Const(segment.offset).i32Const(segment.length).call(importIndex);
            return;
        }
        case 'jsonRaw':
            emitExpr(e, node.expr, scope, ctx);
            return;
        case 'var': {
            const local = scope.get(node.name);
            if (!local) throw new Error(`Tac worker: unknown variable '${node.name}'`);
            e.localGet(local.index);
            return;
        }
        case 'unary':
            if (node.op === '!') {
                emitConditionValue(e, node.expr, scope, ctx);
                e.i32Eqz();
                return;
            }
            e.i32Const(0);
            emitExpr(e, node.expr, scope, ctx);
            e.i32Sub();
            return;
        case 'binary': {
            if (node.type === 'str') {
                emitStringOperand(e, node.left, scope, ctx);
                emitStringOperand(e, node.right, scope, ctx);
                e.call(ctx.fns.strCat);
                return;
            }
            if (LOGICAL_OPS.has(node.op)) {
                emitConditionValue(e, node.left, scope, ctx);
                emitConditionValue(e, node.right, scope, ctx);
                if (node.op === '&&') e.i32And();
                else e.i32Or();
                return;
            }
            emitExpr(e, node.left, scope, ctx);
            emitExpr(e, node.right, scope, ctx);
            // Division and remainder go through trap-safe helpers (x / 0 -> 0).
            if (node.op === '/') { e.call(ctx.fns.idiv); return; }
            if (node.op === '%') { e.call(ctx.fns.imod); return; }
            const apply = /** @type {Record<string, (e: Emitter) => Emitter>} */ (BINARY_OPS)[node.op];
            if (!apply)
                throw new Error(`Tac worker: unsupported operator '${node.op}'`);
            apply(e);
            return;
        }
        case 'if':
            emitConditionValue(e, node.cond, scope, ctx);
            e.if(VAL.i32);
            emitBlockValue(e, node.then, scope, ctx);
            e.else();
            emitBlockValue(e, node.otherwise, scope, ctx);
            e.end();
            return;
        default:
            throw new Error(`Tac worker: expression kind '${node.kind}' cannot produce a value`);
    }
}

/**
 * Emit a condition as a normalized i32 boolean (`0` or `1`).
 * @param {Emitter} e
 * @param {Node} node
 * @param {LocalScope} scope
 * @param {import('./tac-wasm-compiler.js').WorkerRuntimeContext} ctx
 */
function emitConditionValue(e, node, scope, ctx) {
    emitExpr(e, node, scope, ctx);
    if (node.type === 'i32')
        e.i32Eqz().i32Eqz();
}

/**
 * Emit an expression as a string header, coercing an i32 value via `intToStr`.
 * @param {Emitter} e
 * @param {Node} node
 * @param {LocalScope} scope
 * @param {import('./tac-wasm-compiler.js').WorkerRuntimeContext} ctx
 */
function emitStringOperand(e, node, scope, ctx) {
    emitExpr(e, node, scope, ctx);
    if (node.type === 'i32')
        e.call(ctx.fns.intToStr);
}

/**
 * Emit a statement that yields no value.
 * @param {Emitter} e
 * @param {Node} stmt
 * @param {LocalScope} scope
 * @param {import('./tac-wasm-compiler.js').WorkerRuntimeContext} ctx
 */
function emitVoidStmt(e, stmt, scope, ctx) {
    switch (stmt.kind) {
        case 'let':
        case 'assign': {
            const local = scope.get(stmt.name);
            if (!local) throw new Error(`Tac worker: assignment to unknown variable '${stmt.name}'`);
            emitExpr(e, stmt.expr, scope, ctx);
            e.localSet(local.index);
            return;
        }
        case 'exprStmt':
        case 'tail':
            emitExpr(e, stmt.expr, scope, ctx);
            e.drop();
            return;
        case 'while':
            e.block().loop();
            emitConditionValue(e, stmt.cond, scope, ctx);
            e.i32Eqz().brIf(1);
            for (const inner of stmt.body.stmts)
                emitVoidStmt(e, inner, scope, ctx);
            e.br(0).end().end();
            return;
        default:
            throw new Error(`Tac worker: '${stmt.kind}' is not valid here`);
    }
}

/**
 * Emit a block used in value position: its trailing expression (or final
 * `return`) is left on the stack as the block's value.
 * @param {Emitter} e
 * @param {Node} block
 * @param {LocalScope} scope
 * @param {import('./tac-wasm-compiler.js').WorkerRuntimeContext} ctx
 */
function emitBlockValue(e, block, scope, ctx) {
    const stmts = block.stmts;
    if (stmts.length === 0)
        throw new Error('Tac worker: a block in value position must end with an expression');
    for (let i = 0; i < stmts.length - 1; i++) {
        if (stmts[i].kind === 'tail')
            throw new Error('Tac worker: a trailing expression must be the last statement of a block');
        emitVoidStmt(e, stmts[i], scope, ctx);
    }
    const last = stmts[stmts.length - 1];
    if (last.kind === 'tail' || last.kind === 'return')
        emitExpr(e, last.expr, scope, ctx);
    else
        throw new Error('Tac worker: a method/block must end with an expression value');
}

const RESPONSE_PREFIX = '{"status":200,"headers":{"Content-Type":"application/json"},"body":{"method":"';
const RESPONSE_INT_MID = '","result":';
const RESPONSE_INT_SUFFIX = '}}';
const RESPONSE_STR_MID = '","result":"';
const RESPONSE_STR_SUFFIX = '"}}';
const RESPONSE_NOT_FOUND = '{"status":404,"headers":{"Content-Type":"application/json"},"body":{"error":"unknown method"}}';

/**
 * Compile a parsed handler program into worker-ABI wasm bytes. Frontends parse
 * their own syntax into the shared AST and hand it here.
 * @param {{ methods: Node[] }} program
 * @returns {Uint8Array}
 */
export function compileHandlerProgram(program) {
    if (program.methods.length === 0)
        throw new Error('Tac worker: the handler must declare at least one HTTP request method');

    const module = buildWorkerModule((ctx) => {
        const { module: mod, globals, fns, intern } = ctx;

        // streq(aPtr, aLen, bPtr, bLen) -> 1 if byte-equal else 0
        const streq = mod.addFunction({
            params: [VAL.i32, VAL.i32, VAL.i32, VAL.i32],
            results: [VAL.i32],
            locals: [VAL.i32], // i = local 4
            body: new Emitter()
                .localGet(1).localGet(3).i32Ne().if()
                .i32Const(0).return()
                .end()
                .i32Const(0).localSet(4)
                .block().loop()
                .localGet(4).localGet(1).i32GeS().brIf(1)
                .localGet(0).localGet(4).i32Add().i32Load8U()
                .localGet(2).localGet(4).i32Add().i32Load8U()
                .i32Ne().if().i32Const(0).return().end()
                .localGet(4).i32Const(1).i32Add().localSet(4)
                .br(0)
                .end().end()
                .i32Const(1),
        });

        // One wasm function per handler method: (requestPtr, requestLen) -> i32
        // (an i32 value, or - for String methods - a string header pointer).
        const methods = program.methods.map((method) => {
            const { scope, valueType } = analyzeMethod(method);
            const body = new Emitter();
            emitBlockValue(body, method.body, scope, ctx);
            const index = mod.addFunction({
                params: [VAL.i32, VAL.i32],
                results: [VAL.i32],
                locals: Array.from(scope.values()).map(() => VAL.i32),
                body,
            });
            return {
                index,
                valueType,
                quoted: intern(JSON.stringify(method.name)), // "VERB" as it arrives over the ABI
                bare: intern(method.name),
            };
        });

        const prefix = intern(RESPONSE_PREFIX);
        const intMid = intern(RESPONSE_INT_MID);
        const intSuffix = intern(RESPONSE_INT_SUFFIX);
        const strMid = intern(RESPONSE_STR_MID);
        const strSuffix = intern(RESPONSE_STR_SUFFIX);
        const trueValue = intern('true');
        const falseValue = intern('false');
        const notFound = intern(RESPONSE_NOT_FOUND);

        // call params: methodPtr=0, methodLen=1, requestPtr=2, requestLen=3, bodyPtr=4, bodyLen=5.
        // Dispatcher locals: out=6, pos=7, n=8, result=9.
        const e = new Emitter();
        // Publish the request body so request.body() can read it during dispatch.
        e.localGet(4).globalSet(globals.bodyPtr);
        e.localGet(5).globalSet(globals.bodyLen);

        // Single-exit dispatch: a matched verb writes its response and `br`s out
        // of this block (rather than `return`-ing) so the worker runtime's
        // end-of-call heap reset always runs. No match falls through to the 404.
        e.block();

        /** @param {{ offset: number, length: number }} segment */
        const copyConst = (segment) => e
            .localGet(7).i32Const(segment.offset).i32Const(segment.length).call(fns.copy).localSet(8)
            .localGet(7).localGet(8).i32Add().localSet(7);

        for (const method of methods) {
            e.localGet(0).localGet(1).i32Const(method.quoted.offset).i32Const(method.quoted.length).call(streq).if();
            e.localGet(2).localGet(3).call(method.index).localSet(9); // result = method(requestPtr, requestLen)
            if (method.valueType === 'str') {
                // out = alloc(256 + 2 * resultLen) for worst-case JSON escaping.
                e.localGet(9).i32Load(4).i32Const(2).i32Mul().i32Const(256).i32Add().call(fns.alloc).localSet(6);
                e.localGet(6).localSet(7);
                copyConst(prefix);
                copyConst(method.bare);
                copyConst(strMid);
                e.localGet(9).localGet(7).call(fns.jsonEscape).localSet(8)
                    .localGet(7).localGet(8).i32Add().localSet(7);
                copyConst(strSuffix);
            }
            else if (method.valueType === 'json') {
                // Json results are already valid JSON bytes; copy them into
                // body.result without wrapping them in quotes.
                e.localGet(9).i32Load(4).i32Const(256).i32Add().call(fns.alloc).localSet(6);
                e.localGet(6).localSet(7);
                copyConst(prefix);
                copyConst(method.bare);
                copyConst(intMid);
                e.localGet(7).localGet(9).i32Load(0).localGet(9).i32Load(4).call(fns.copy).localSet(8)
                    .localGet(7).localGet(8).i32Add().localSet(7);
                copyConst(intSuffix);
            }
            else if (method.valueType === 'bool') {
                e.i32Const(256).call(fns.alloc).localSet(6);
                e.localGet(6).localSet(7);
                copyConst(prefix);
                copyConst(method.bare);
                copyConst(intMid);
                e.localGet(9).if();
                copyConst(trueValue);
                e.else();
                copyConst(falseValue);
                e.end();
                copyConst(intSuffix);
            }
            else {
                e.i32Const(256).call(fns.alloc).localSet(6);
                e.localGet(6).localSet(7);
                copyConst(prefix);
                copyConst(method.bare);
                copyConst(intMid);
                e.localGet(9).localGet(7).call(fns.itoa).localSet(8)
                    .localGet(7).localGet(8).i32Add().localSet(7);
                copyConst(intSuffix);
            }
            e.localGet(6).globalSet(globals.outPtr);
            e.localGet(7).localGet(6).i32Sub().globalSet(globals.outLen);
            e.br(1); // break out of the dispatch block (skips the 404 fallback)
            e.end();
        }

        // Fallback: unknown verb -> 404.
        e.i32Const(128).call(fns.alloc).localSet(6);
        e.localGet(6).i32Const(notFound.offset).i32Const(notFound.length).call(fns.copy).localSet(8);
        e.localGet(6).globalSet(globals.outPtr);
        e.localGet(8).globalSet(globals.outLen);
        e.end(); // close the dispatch block
        return { body: e, locals: [VAL.i32, VAL.i32, VAL.i32, VAL.i32] };
    });

    return module.toBytes();
}
