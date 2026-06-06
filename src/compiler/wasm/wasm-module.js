// @ts-check
//
// Hand-written WebAssembly binary encoder.
//
// This is the language-agnostic backend of Tachyon's in-house Tac Worker
// compiler. It emits a valid `.wasm` module (MVP binary format) with zero
// external toolchain - no rustc, clang, or binaryen. A frontend lexer/parser
// lowers handler-shaped source into calls against `WasmModule` + `Emitter`,
// and `WasmModule.toBytes()` produces the bytes that instantiate in any engine.
//
// Reference: https://webassembly.github.io/spec/core/binary/index.html

/** WebAssembly value types (binary encoding). */
export const VAL = Object.freeze({
    i32: 0x7f,
    i64: 0x7e,
    f32: 0x7d,
    f64: 0x7c,
});

/** Block type for structured control flow with no result. */
export const EMPTY_BLOCK = 0x40;

const SECTION = Object.freeze({
    type: 1,
    import: 2,
    func: 3,
    table: 4,
    memory: 5,
    global: 6,
    export: 7,
    start: 8,
    element: 9,
    code: 10,
    data: 11,
});

/**
 * Encode an unsigned integer as unsigned LEB128.
 * @param {number} value
 * @returns {number[]}
 */
export function unsignedLEB(value) {
    if (!Number.isInteger(value) || value < 0)
        throw new Error(`unsignedLEB expects a non-negative integer, received ${value}`);
    const out = [];
    let remaining = value;
    do {
        let byte = remaining & 0x7f;
        remaining = Math.floor(remaining / 128);
        if (remaining !== 0)
            byte |= 0x80;
        out.push(byte);
    } while (remaining !== 0);
    return out;
}

/**
 * Encode a signed integer as signed LEB128.
 * @param {number} value
 * @returns {number[]}
 */
export function signedLEB(value) {
    if (!Number.isInteger(value))
        throw new Error(`signedLEB expects an integer, received ${value}`);
    const out = [];
    let more = true;
    let remaining = value;
    while (more) {
        let byte = remaining & 0x7f;
        // Arithmetic shift toward negative infinity (matches a signed >> 7).
        remaining = Math.floor(remaining / 128);
        const signBit = (byte & 0x40) !== 0;
        if ((remaining === 0 && !signBit) || (remaining === -1 && signBit))
            more = false;
        else
            byte |= 0x80;
        out.push(byte);
    }
    return out;
}

/**
 * Encode a 64-bit float as 8 little-endian bytes (IEEE 754).
 * @param {number} value
 * @returns {number[]}
 */
export function f64Bytes(value) {
    const buffer = new ArrayBuffer(8);
    new DataView(buffer).setFloat64(0, value, true);
    return Array.from(new Uint8Array(buffer));
}

/**
 * Encode a UTF-8 name: length-prefixed byte vector.
 * @param {string} text
 * @returns {number[]}
 */
export function encodeName(text) {
    const bytes = Array.from(new TextEncoder().encode(text));
    return [...unsignedLEB(bytes.length), ...bytes];
}

/**
 * Wrap a section body in `id + size + body`.
 * @param {number} id
 * @param {number[]} body
 * @returns {number[]}
 */
function section(id, body) {
    if (body.length === 0)
        return [];
    return [id, ...unsignedLEB(body.length), ...body];
}

/**
 * Encode a vector: count-prefixed concatenation of already-encoded items.
 * @param {number[][]} items
 * @returns {number[]}
 */
function vector(items) {
    const out = [...unsignedLEB(items.length)];
    for (const item of items)
        out.push(...item);
    return out;
}

/**
 * Fluent emitter for a single function body. Each method appends the encoded
 * instruction bytes and returns `this` for chaining. Call `finish()` to append
 * the trailing `end` and obtain the raw body bytes.
 */
export class Emitter {
    constructor() {
        /** @type {number[]} */
        this.bytes = [];
    }

    /** @param {number[]} bytes @returns {this} */
    raw(bytes) {
        this.bytes.push(...bytes);
        return this;
    }

    // --- control flow ---
    /** @param {number} [blockType] @returns {this} */
    block(blockType = EMPTY_BLOCK) { return this.raw([0x02, blockType]); }
    /** @param {number} [blockType] @returns {this} */
    loop(blockType = EMPTY_BLOCK) { return this.raw([0x03, blockType]); }
    /** @param {number} [blockType] @returns {this} */
    if(blockType = EMPTY_BLOCK) { return this.raw([0x04, blockType]); }
    /** @returns {this} */
    else() { return this.raw([0x05]); }
    /** @returns {this} */
    end() { return this.raw([0x0b]); }
    /** @param {number} depth @returns {this} */
    br(depth) { return this.raw([0x0c, ...unsignedLEB(depth)]); }
    /** @param {number} depth @returns {this} */
    brIf(depth) { return this.raw([0x0d, ...unsignedLEB(depth)]); }
    /** @returns {this} */
    return() { return this.raw([0x0f]); }
    /** @returns {this} */
    unreachable() { return this.raw([0x00]); }
    /** @returns {this} */
    nop() { return this.raw([0x01]); }
    /** @returns {this} */
    drop() { return this.raw([0x1a]); }
    /** @param {number} funcIndex @returns {this} */
    call(funcIndex) { return this.raw([0x10, ...unsignedLEB(funcIndex)]); }

    // --- locals / globals ---
    /** @param {number} index @returns {this} */
    localGet(index) { return this.raw([0x20, ...unsignedLEB(index)]); }
    /** @param {number} index @returns {this} */
    localSet(index) { return this.raw([0x21, ...unsignedLEB(index)]); }
    /** @param {number} index @returns {this} */
    localTee(index) { return this.raw([0x22, ...unsignedLEB(index)]); }
    /** @param {number} index @returns {this} */
    globalGet(index) { return this.raw([0x23, ...unsignedLEB(index)]); }
    /** @param {number} index @returns {this} */
    globalSet(index) { return this.raw([0x24, ...unsignedLEB(index)]); }

    // --- constants ---
    /** @param {number} value @returns {this} */
    i32Const(value) { return this.raw([0x41, ...signedLEB(value)]); }
    /** @param {number} value @returns {this} */
    i64Const(value) { return this.raw([0x42, ...signedLEB(value)]); }
    /** @param {number} value @returns {this} */
    f64Const(value) { return this.raw([0x44, ...f64Bytes(value)]); }

    // --- memory (align is log2 of byte alignment) ---
    /** @param {number} [offset] @param {number} [align] @returns {this} */
    i32Load(offset = 0, align = 2) { return this.raw([0x28, ...unsignedLEB(align), ...unsignedLEB(offset)]); }
    /** @param {number} [offset] @returns {this} */
    i32Load8U(offset = 0) { return this.raw([0x2d, ...unsignedLEB(0), ...unsignedLEB(offset)]); }
    /** @param {number} [offset] @param {number} [align] @returns {this} */
    i32Store(offset = 0, align = 2) { return this.raw([0x36, ...unsignedLEB(align), ...unsignedLEB(offset)]); }
    /** @param {number} [offset] @returns {this} */
    i32Store8(offset = 0) { return this.raw([0x3a, ...unsignedLEB(0), ...unsignedLEB(offset)]); }
    /** @returns {this} */
    memorySize() { return this.raw([0x3f, 0x00]); }
    /** @returns {this} */
    memoryGrow() { return this.raw([0x40, 0x00]); }

    // --- i32 comparisons ---
    /** @returns {this} */ i32Eqz() { return this.raw([0x45]); }
    /** @returns {this} */ i32Eq() { return this.raw([0x46]); }
    /** @returns {this} */ i32Ne() { return this.raw([0x47]); }
    /** @returns {this} */ i32LtS() { return this.raw([0x48]); }
    /** @returns {this} */ i32LtU() { return this.raw([0x49]); }
    /** @returns {this} */ i32GtS() { return this.raw([0x4a]); }
    /** @returns {this} */ i32GtU() { return this.raw([0x4b]); }
    /** @returns {this} */ i32LeS() { return this.raw([0x4c]); }
    /** @returns {this} */ i32GeS() { return this.raw([0x4e]); }

    // --- i32 arithmetic / bitwise ---
    /** @returns {this} */ i32Add() { return this.raw([0x6a]); }
    /** @returns {this} */ i32Sub() { return this.raw([0x6b]); }
    /** @returns {this} */ i32Mul() { return this.raw([0x6c]); }
    /** @returns {this} */ i32DivS() { return this.raw([0x6d]); }
    /** @returns {this} */ i32DivU() { return this.raw([0x6e]); }
    /** @returns {this} */ i32RemS() { return this.raw([0x6f]); }
    /** @returns {this} */ i32RemU() { return this.raw([0x70]); }
    /** @returns {this} */ i32And() { return this.raw([0x71]); }
    /** @returns {this} */ i32Or() { return this.raw([0x72]); }
    /** @returns {this} */ i32Xor() { return this.raw([0x73]); }
    /** @returns {this} */ i32Shl() { return this.raw([0x74]); }
    /** @returns {this} */ i32ShrS() { return this.raw([0x75]); }
    /** @returns {this} */ i32ShrU() { return this.raw([0x76]); }

    /**
     * Finalize the body: append the terminating `end` opcode.
     * @returns {number[]}
     */
    finish() {
        return [...this.bytes, 0x0b];
    }
}

/**
 * @typedef {object} WasmFunctionSpec
 * @property {string} [name] Export name; omit to keep the function internal.
 * @property {number[]} [params] Parameter value types.
 * @property {number[]} [results] Result value types.
 * @property {number[]} [locals] Additional local value types (beyond params).
 * @property {Emitter | number[]} body Instruction body (Emitter is auto-finished).
 */

/**
 * Builder for a complete WebAssembly module. Functions are appended in order;
 * the first added function has index 0 (this encoder declares no imported
 * functions, so function indices equal definition order).
 */
export class WasmModule {
    constructor() {
        /** @type {{ params: number[], results: number[] }[]} */
        this.types = [];
        /** @type {{ module: string, name: string, typeIndex: number }[]} */
        this.imports = [];
        /** @type {{ typeIndex: number, locals: number[], body: number[], name?: string }[]} */
        this.functions = [];
        /** @type {{ name: string, index: number, kind: number }[]} */
        this.exports = [];
        /** @type {{ min: number, max?: number, exportName?: string } | null} */
        this.memory = null;
        /** @type {{ type: number, mutable: boolean, init: number }[]} */
        this.globals = [];
        /** @type {{ offset: number, bytes: number[] }[]} */
        this.dataSegments = [];
    }

    /**
     * Declare a module global. Returns its global index.
     * @param {{ type: number, mutable?: boolean, init?: number }} spec
     * @returns {number}
     */
    addGlobal(spec) {
        this.globals.push({ type: spec.type, mutable: spec.mutable ?? true, init: spec.init ?? 0 });
        return this.globals.length - 1;
    }

    /**
     * Declare (and export) linear memory.
     * @param {number} minPages
     * @param {number} [maxPages]
     * @param {string} [exportName]
     * @returns {this}
     */
    setMemory(minPages, maxPages, exportName = 'memory') {
        this.memory = { min: minPages, max: maxPages, exportName };
        return this;
    }

    /**
     * Intern a function type, returning its type index.
     * @param {number[]} params
     * @param {number[]} results
     * @returns {number}
     */
    internType(params, results) {
        const existing = this.types.findIndex(
            (type) => type.params.length === params.length
                && type.results.length === results.length
                && type.params.every((value, i) => value === params[i])
                && type.results.every((value, i) => value === results[i]),
        );
        if (existing !== -1)
            return existing;
        this.types.push({ params: [...params], results: [...results] });
        return this.types.length - 1;
    }

    /**
     * Declare an imported function. Imported functions occupy the lowest slots
     * of the function index space, so every `addImportFunction` MUST be called
     * before any `addFunction` whose returned index depends on the import count.
     * @param {{ module: string, name: string, params?: number[], results?: number[] }} spec
     * @returns {number} The import's function index (equal to its declaration order).
     */
    addImportFunction(spec) {
        const typeIndex = this.internType(spec.params ?? [], spec.results ?? []);
        const index = this.imports.length;
        this.imports.push({ module: spec.module, name: spec.name, typeIndex });
        return index;
    }

    /**
     * Append a function definition. Returns its function index (offset past any
     * imported functions, which occupy the lowest indices).
     * @param {WasmFunctionSpec} spec
     * @returns {number}
     */
    addFunction(spec) {
        const params = spec.params ?? [];
        const results = spec.results ?? [];
        const locals = spec.locals ?? [];
        const body = spec.body instanceof Emitter ? spec.body.finish() : spec.body;
        const typeIndex = this.internType(params, results);
        const funcIndex = this.imports.length + this.functions.length;
        this.functions.push({ typeIndex, locals, body, name: spec.name });
        if (spec.name)
            this.exports.push({ name: spec.name, index: funcIndex, kind: 0x00 });
        return funcIndex;
    }

    /**
     * Add an active data segment placed at a constant i32 offset.
     * @param {number} offset
     * @param {number[] | Uint8Array} bytes
     * @returns {this}
     */
    addData(offset, bytes) {
        this.dataSegments.push({ offset, bytes: Array.from(bytes) });
        return this;
    }

    /**
     * Compress an additional-locals list into (count, type) runs.
     * @param {number[]} locals
     * @returns {number[]}
     */
    static encodeLocals(locals) {
        /** @type {[number, number][]} */
        const runs = [];
        for (const type of locals) {
            const last = runs[runs.length - 1];
            if (last && last[1] === type)
                last[0] += 1;
            else
                runs.push([1, type]);
        }
        return vector(runs.map(([count, type]) => [...unsignedLEB(count), type]));
    }

    /**
     * Serialize the module to its binary form.
     * @returns {Uint8Array}
     */
    toBytes() {
        const out = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

        // Type section
        out.push(...section(SECTION.type, vector(
            this.types.map((type) => [0x60, ...vector(type.params.map((p) => [p])), ...vector(type.results.map((r) => [r]))]),
        )));

        // Import section (imported functions occupy the lowest function indices)
        if (this.imports.length > 0) {
            out.push(...section(SECTION.import, vector(
                this.imports.map((imp) => [
                    ...encodeName(imp.module),
                    ...encodeName(imp.name),
                    0x00, // import kind: function
                    ...unsignedLEB(imp.typeIndex),
                ]),
            )));
        }

        // Function section (type index per defined function)
        out.push(...section(SECTION.func, vector(
            this.functions.map((fn) => unsignedLEB(fn.typeIndex)),
        )));

        // Memory section
        if (this.memory) {
            const limits = this.memory.max === undefined
                ? [0x00, ...unsignedLEB(this.memory.min)]
                : [0x01, ...unsignedLEB(this.memory.min), ...unsignedLEB(this.memory.max)];
            out.push(...section(SECTION.memory, vector([limits])));
        }

        // Global section
        if (this.globals.length > 0) {
            out.push(...section(SECTION.global, vector(
                this.globals.map((global) => {
                    const init = global.type === VAL.f64
                        ? [0x44, ...f64Bytes(global.init), 0x0b]
                        : global.type === VAL.i64
                            ? [0x42, ...signedLEB(global.init), 0x0b]
                            : [0x41, ...signedLEB(global.init), 0x0b];
                    return [global.type, global.mutable ? 0x01 : 0x00, ...init];
                }),
            )));
        }

        // Export section (functions + memory)
        /** @type {number[][]} */
        const exportEntries = this.exports.map((entry) => [...encodeName(entry.name), entry.kind, ...unsignedLEB(entry.index)]);
        if (this.memory?.exportName)
            exportEntries.push([...encodeName(this.memory.exportName), 0x02, ...unsignedLEB(0)]);
        out.push(...section(SECTION.export, vector(exportEntries)));

        // Code section
        out.push(...section(SECTION.code, vector(
            this.functions.map((fn) => {
                const code = [...WasmModule.encodeLocals(fn.locals), ...fn.body];
                return [...unsignedLEB(code.length), ...code];
            }),
        )));

        // Data section
        if (this.dataSegments.length > 0) {
            out.push(...section(SECTION.data, vector(
                this.dataSegments.map((segment) => [
                    0x00, // active segment, memory 0
                    0x41, ...signedLEB(segment.offset), 0x0b, // i32.const offset; end
                    ...unsignedLEB(segment.bytes.length), ...segment.bytes,
                ]),
            )));
        }

        return new Uint8Array(out);
    }
}
