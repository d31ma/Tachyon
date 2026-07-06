// @ts-check
//
// In-house Tac Worker -> Wasm runtime + codegen.
//
// `buildWorkerModule()` synthesizes the worker ABI that Tachyon's generated
// `tac.worker.js` expects - exported `memory`, `alloc`, `dealloc`, `call`,
// `output_ptr`, `output_len` - entirely from `WasmModule`/`Emitter`, with no
// external toolchain. A frontend lowers handler-shaped source into the `call`
// body via `emitCall`; this module owns the shared runtime (a bump allocator,
// a byte-copy, and `itoa`) so every language frontend reuses identical glue.

import { Emitter, VAL, WasmModule } from './wasm-module.js';

/** Linear-memory address where the bump allocator starts handing out memory. */
export const HEAP_BASE = 4096;
/** Linear-memory address where interned string constants are placed. */
export const DATA_BASE = 16;

/**
 * @typedef {object} WorkerRuntimeContext
 * @property {WasmModule} module
 * @property {{ heap?: number, outPtr: number, outLen: number, bodyPtr: number, bodyLen: number }} globals Global indices.
 * @property {{ copy: number, itoa: number, alloc: number, mkStr: number, intToStr: number, strCat: number, jsonEscape: number, quoteStr: number, idiv: number, imod: number }} fns Internal helper function indices. String values are pointers to an `{ dataPtr@0, byteLen@4 }` header.
 * @property {{ query: number, path: number, header: number, platform: number } | null} imports Host-provided request-field import indices (`req_query`/`req_path`/`req_header`/`req_platform`), or null when the handler does not read request fields. Each takes `(keyPtr, keyLen)` and returns a string-header pointer.
 * @property {(text: string) => { offset: number, length: number }} intern Place a UTF-8 constant in memory.
 */

/**
 * Build a complete Tac Worker wasm module around a caller-supplied `call` body.
 * `emitCall` returns either an Emitter (default `call` locals: out, pos, n) or
 * `{ body, locals }` to declare additional `call` locals (indices start at 4,
 * after the four ABI parameters).
 * @param {(context: WorkerRuntimeContext) => (Emitter | { body: Emitter, locals: number[] })} emitCall
 * @param {{ requestFields?: boolean }} [options] `requestFields: true` declares the host-provided
 *   `req_query`/`req_path`/`req_header` imports (used when the handler reads request fields).
 * @returns {WasmModule}
 */
export function buildWorkerModule(emitCall, options = {}) {
    const module = new WasmModule();
    module.setMemory(2); // 2 pages (128 KiB)

    // Host-provided request-field accessors. Declared FIRST so imports occupy
    // the lowest function indices; only when the handler actually reads request
    // fields, so handlers that don't stay import-free (instantiate with `{}`).
    const imports = options.requestFields
        ? {
            query: module.addImportFunction({ module: 'env', name: 'req_query', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
            path: module.addImportFunction({ module: 'env', name: 'req_path', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
            header: module.addImportFunction({ module: 'env', name: 'req_header', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
            platform: module.addImportFunction({ module: 'env', name: 'req_platform', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
        }
        : null;

    const globals = {
        heap: module.addGlobal({ type: VAL.i32, mutable: true, init: HEAP_BASE }),
        outPtr: module.addGlobal({ type: VAL.i32, mutable: true, init: 0 }),
        outLen: module.addGlobal({ type: VAL.i32, mutable: true, init: 0 }),
        // Set by the dispatcher each call so `request.body()` can read it.
        bodyPtr: module.addGlobal({ type: VAL.i32, mutable: true, init: 0 }),
        bodyLen: module.addGlobal({ type: VAL.i32, mutable: true, init: 0 }),
    };

    // copy(dst, src, len) -> len : byte-wise memory copy.
    const copy = module.addFunction({
        params: [VAL.i32, VAL.i32, VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32], // i = local 3
        body: new Emitter()
            .block().loop()
            .localGet(3).localGet(2).i32GeS().brIf(1)
            .localGet(0).localGet(3).i32Add()
            .localGet(1).localGet(3).i32Add().i32Load8U()
            .i32Store8()
            .localGet(3).i32Const(1).i32Add().localSet(3)
            .br(0)
            .end().end()
            .localGet(2),
    });

    // itoa(value, dst) -> bytesWritten : decimal-encode a signed i32 (handles a
    // leading '-' for negatives). Magnitude digits use *unsigned* division so
    // INT_MIN (whose two's-complement negation is itself) renders correctly.
    const itoa = module.addFunction({
        params: [VAL.i32, VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32, VAL.i32, VAL.i32, VAL.i32], // digits=2, n=3, i=4, start=5
        body: new Emitter()
            .localGet(0).i32Eqz().if()
            .localGet(1).i32Const(48).i32Store8()
            .i32Const(1).return()
            .end()
            // start = dst; a negative value emits '-' then continues with |value|.
            .localGet(1).localSet(5)
            .localGet(0).i32Const(0).i32LtS().if()
            .localGet(1).i32Const(45).i32Store8() // '-'
            .localGet(1).i32Const(1).i32Add().localSet(5) // start = dst + 1
            .i32Const(0).localGet(0).i32Sub().localSet(0) // value = -value (bit pattern = |value|)
            .end()
            // digits = base-10 length of the unsigned magnitude.
            .localGet(0).localSet(3)
            .i32Const(0).localSet(2)
            .block().loop()
            .localGet(3).i32Eqz().brIf(1)
            .localGet(2).i32Const(1).i32Add().localSet(2)
            .localGet(3).i32Const(10).i32DivU().localSet(3)
            .br(0)
            .end().end()
            // write digits right-to-left into start..start+digits.
            .localGet(0).localSet(3)
            .localGet(2).localSet(4)
            .block().loop()
            .localGet(3).i32Eqz().brIf(1)
            .localGet(4).i32Const(1).i32Sub().localSet(4)
            .localGet(5).localGet(4).i32Add()
            .localGet(3).i32Const(10).i32RemU().i32Const(48).i32Add()
            .i32Store8()
            .localGet(3).i32Const(10).i32DivU().localSet(3)
            .br(0)
            .end().end()
            // bytesWritten = sign + digits = (start + digits) - dst.
            .localGet(5).localGet(2).i32Add().localGet(1).i32Sub(),
    });

    // alloc(size) -> ptr : 8-byte-aligned bump allocator that grows linear memory
    // on demand, so a single oversized request/response can exceed the initial
    // 2-page heap instead of trapping on an out-of-bounds store.
    const alloc = module.addFunction({
        name: 'alloc',
        params: [VAL.i32], // size = local 0
        results: [VAL.i32],
        locals: [VAL.i32, VAL.i32], // ptr = local 1, neededPages = local 2
        body: new Emitter()
            .globalGet(globals.heap).localSet(1) // ptr = heap
            // heap = (heap + size + 7) & ~7
            .globalGet(globals.heap).localGet(0).i32Add().i32Const(7).i32Add().i32Const(-8).i32And()
            .globalSet(globals.heap)
            // neededPages = ceil(heap / 64KiB) = (heap + 65535) >>> 16
            .globalGet(globals.heap).i32Const(65535).i32Add().i32Const(16).i32ShrU().localSet(2)
            // if neededPages > memory.size: memory.grow(neededPages - memory.size)
            .localGet(2).memorySize().i32GtS().if()
            .localGet(2).memorySize().i32Sub().memoryGrow().drop()
            .end()
            .localGet(1),
    });

    // A string value is a pointer to an 8-byte header: { dataPtr@0, byteLen@4 }.
    // mkStr(dataPtr, len) -> header : wrap an existing byte range as a string.
    const mkStr = module.addFunction({
        params: [VAL.i32, VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32], // header = local 2
        body: new Emitter()
            .i32Const(8).call(alloc).localSet(2)
            .localGet(2).localGet(0).i32Store(0)
            .localGet(2).localGet(1).i32Store(4)
            .localGet(2),
    });

    // intToStr(value) -> header : decimal string for a non-negative i32.
    const intToStr = module.addFunction({
        params: [VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32, VAL.i32], // buf=1, len=2
        body: new Emitter()
            .i32Const(12).call(alloc).localSet(1)
            .localGet(0).localGet(1).call(itoa).localSet(2)
            .localGet(1).localGet(2).call(mkStr),
    });

    // strCat(a, b) -> header : concatenate two strings into a fresh buffer.
    const strCat = module.addFunction({
        params: [VAL.i32, VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32, VAL.i32, VAL.i32, VAL.i32, VAL.i32], // aPtr=2,aLen=3,bPtr=4,bLen=5,buf=6
        body: new Emitter()
            .localGet(0).i32Load(0).localSet(2)
            .localGet(0).i32Load(4).localSet(3)
            .localGet(1).i32Load(0).localSet(4)
            .localGet(1).i32Load(4).localSet(5)
            .localGet(3).localGet(5).i32Add().call(alloc).localSet(6)
            .localGet(6).localGet(2).localGet(3).call(copy).drop()
            .localGet(6).localGet(3).i32Add().localGet(4).localGet(5).call(copy).drop()
            .localGet(6).localGet(3).localGet(5).i32Add().call(mkStr),
    });

    // jsonEscape(strHeader, dest) -> bytesWritten : write the string into `dest`
    // as JSON-safe bytes. `"` and `\` and the whitespace control chars
    // (\b \t \n \f \r) become two-byte backslash escapes; any other control
    // char (< 0x20) falls back to a space. Every byte expands to at most 2, so
    // the dispatcher's `2*len + 256` scratch is always sufficient.
    const jsonEscape = module.addFunction({
        params: [VAL.i32, VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32, VAL.i32, VAL.i32, VAL.i32, VAL.i32, VAL.i32], // ptr=2,len=3,i=4,pos=5,ch=6,esc=7
        body: new Emitter()
            .localGet(0).i32Load(0).localSet(2)
            .localGet(0).i32Load(4).localSet(3)
            .i32Const(0).localSet(4)
            .localGet(1).localSet(5)
            .block().loop()
            .localGet(4).localGet(3).i32GeS().brIf(1)
            .localGet(2).localGet(4).i32Add().i32Load8U().localSet(6)
            // esc = the escape character to follow a backslash, or 0 for none.
            .i32Const(0).localSet(7)
            .localGet(6).i32Const(34).i32Eq().if().i32Const(34).localSet(7).end() // "  -> \"
            .localGet(6).i32Const(92).i32Eq().if().i32Const(92).localSet(7).end() // \  -> \\
            .localGet(6).i32Const(8).i32Eq().if().i32Const(98).localSet(7).end() // \b -> \b
            .localGet(6).i32Const(9).i32Eq().if().i32Const(116).localSet(7).end() // \t -> \t
            .localGet(6).i32Const(10).i32Eq().if().i32Const(110).localSet(7).end() // \n -> \n
            .localGet(6).i32Const(12).i32Eq().if().i32Const(102).localSet(7).end() // \f -> \f
            .localGet(6).i32Const(13).i32Eq().if().i32Const(114).localSet(7).end() // \r -> \r
            .localGet(7).if()
            .localGet(5).i32Const(92).i32Store8()
            .localGet(5).i32Const(1).i32Add().localGet(7).i32Store8()
            .localGet(5).i32Const(2).i32Add().localSet(5)
            .else()
            .localGet(6).i32Const(32).i32LtS().if()
            .localGet(5).i32Const(32).i32Store8()
            .localGet(5).i32Const(1).i32Add().localSet(5)
            .else()
            .localGet(5).localGet(6).i32Store8()
            .localGet(5).i32Const(1).i32Add().localSet(5)
            .end()
            .end()
            .localGet(4).i32Const(1).i32Add().localSet(4)
            .br(0)
            .end().end()
            .localGet(5).localGet(1).i32Sub(),
    });

    // quoteStr(strHeader) -> header : render a string as a JSON string value -
    // a leading `"`, the JSON-escaped bytes, and a trailing `"`. Used when an
    // object literal embeds a String value. jsonEscape expands each byte to at
    // most 2, so `2*len + 2` scratch always suffices.
    const quoteStr = module.addFunction({
        params: [VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32, VAL.i32, VAL.i32], // len=1, buf=2, n=3
        body: new Emitter()
            .localGet(0).i32Load(4).localSet(1)
            .localGet(1).i32Const(2).i32Mul().i32Const(2).i32Add().call(alloc).localSet(2)
            .localGet(2).i32Const(34).i32Store8() // buf[0] = '"'
            .localGet(0).localGet(2).i32Const(1).i32Add().call(jsonEscape).localSet(3) // n = jsonEscape(s, buf+1)
            .localGet(2).i32Const(1).i32Add().localGet(3).i32Add().i32Const(34).i32Store8() // buf[1+n] = '"'
            .localGet(2).localGet(3).i32Const(2).i32Add().call(mkStr),
    });

    // idiv(a, b) -> i32 : signed division that yields 0 on /0 instead of trapping
    // (a hostile or empty request must not be able to kill the worker module).
    const idiv = module.addFunction({
        params: [VAL.i32, VAL.i32],
        results: [VAL.i32],
        body: new Emitter()
            .localGet(1).i32Eqz().if(VAL.i32)
            .i32Const(0)
            .else()
            .localGet(0).localGet(1).i32DivS()
            .end(),
    });

    // imod(a, b) -> i32 : signed remainder that yields 0 on /0 instead of trapping.
    const imod = module.addFunction({
        params: [VAL.i32, VAL.i32],
        results: [VAL.i32],
        body: new Emitter()
            .localGet(1).i32Eqz().if(VAL.i32)
            .i32Const(0)
            .else()
            .localGet(0).localGet(1).i32RemS()
            .end(),
    });

    // dealloc(ptr, len) : no-op (bump allocator; workers are cheap to reinstantiate).
    module.addFunction({ name: 'dealloc', params: [VAL.i32, VAL.i32], results: [], body: new Emitter() });

    module.addFunction({ name: 'output_ptr', results: [VAL.i32], body: new Emitter().globalGet(globals.outPtr) });
    module.addFunction({ name: 'output_len', results: [VAL.i32], body: new Emitter().globalGet(globals.outLen) });

    // Intern string constants into a single data segment placed at DATA_BASE.
    /** @type {number[]} */
    const dataBytes = [];
    /** @param {string} text */
    const intern = (text) => {
        const bytes = Array.from(new TextEncoder().encode(text));
        const offset = DATA_BASE + dataBytes.length;
        dataBytes.push(...bytes);
        return { offset, length: bytes.length };
    };

    const callResult = emitCall({ module, globals, fns: { copy, itoa, alloc, mkStr, intToStr, strCat, jsonEscape, quoteStr, idiv, imod }, imports, intern });
    const callBody = callResult instanceof Emitter ? callResult : callResult.body;
    const callLocals = callResult instanceof Emitter ? [VAL.i32, VAL.i32, VAL.i32] : callResult.locals;

    // Reset the bump allocator to HEAP_BASE at the *end* of every call so memory
    // stays flat across requests instead of leaking until the 2-page heap traps.
    // It must be the end (not the start): the host allocates the call's inputs
    // (method/request/body) into this same arena before calling, so an early
    // reset would let in-call scratch overwrite them. Resetting only moves the
    // bump pointer - the just-written output bytes survive for the host to read
    // via output_ptr/output_len before it allocates the next call's inputs.
    // Requires the `call` body to be single-exit (no early `return`) so the
    // reset always runs; both compileEchoWorker and compileHandlerProgram comply.
    const callWithReset = new Emitter()
        .raw(callBody.bytes)
        .i32Const(HEAP_BASE).globalSet(globals.heap);

    if (DATA_BASE + dataBytes.length > HEAP_BASE)
        throw new Error(`Tac worker constant data (${dataBytes.length} bytes) overflows the heap base (${HEAP_BASE})`);
    if (dataBytes.length > 0)
        module.addData(DATA_BASE, dataBytes);

    module.addFunction({
        name: 'call',
        // methodPtr, methodLen, requestPtr, requestLen, bodyPtr, bodyLen
        params: [VAL.i32, VAL.i32, VAL.i32, VAL.i32, VAL.i32, VAL.i32],
        results: [],
        locals: callLocals, // default out=6, pos=7, n=8 (after the 6 params; frontends may add more)
        body: callWithReset,
    });

    return module;
}

/**
 * Build the shared Tac Worker runtime module.
 * Exports `memory`, `heap` (global), and all utility functions so that
 * per-handler modules can import them instead of duplicating the runtime.
 * @returns {Uint8Array}
 */
export function buildRuntimeModule() {
    const module = new WasmModule();
    module.setMemory(2); // 2 pages (128 KiB)

    const globals = {
        heap: module.addGlobal({ type: VAL.i32, mutable: true, init: HEAP_BASE }),
    };

    // copy(dst, src, len) -> len
    const copy = module.addFunction({
        name: 'copy',
        params: [VAL.i32, VAL.i32, VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32],
        body: new Emitter()
            .block().loop()
            .localGet(3).localGet(2).i32GeS().brIf(1)
            .localGet(0).localGet(3).i32Add()
            .localGet(1).localGet(3).i32Add().i32Load8U()
            .i32Store8()
            .localGet(3).i32Const(1).i32Add().localSet(3)
            .br(0)
            .end().end()
            .localGet(2),
    });

    // itoa(value, dst) -> bytesWritten
    const itoa = module.addFunction({
        name: 'itoa',
        params: [VAL.i32, VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32, VAL.i32, VAL.i32, VAL.i32],
        body: new Emitter()
            .localGet(0).i32Eqz().if()
            .localGet(1).i32Const(48).i32Store8()
            .i32Const(1).return()
            .end()
            .localGet(1).localSet(5)
            .localGet(0).i32Const(0).i32LtS().if()
            .localGet(1).i32Const(45).i32Store8()
            .localGet(1).i32Const(1).i32Add().localSet(5)
            .i32Const(0).localGet(0).i32Sub().localSet(0)
            .end()
            .localGet(0).localSet(3)
            .i32Const(0).localSet(2)
            .block().loop()
            .localGet(3).i32Eqz().brIf(1)
            .localGet(2).i32Const(1).i32Add().localSet(2)
            .localGet(3).i32Const(10).i32DivU().localSet(3)
            .br(0)
            .end().end()
            .localGet(0).localSet(3)
            .localGet(2).localSet(4)
            .block().loop()
            .localGet(3).i32Eqz().brIf(1)
            .localGet(4).i32Const(1).i32Sub().localSet(4)
            .localGet(5).localGet(4).i32Add()
            .localGet(3).i32Const(10).i32RemU().i32Const(48).i32Add()
            .i32Store8()
            .localGet(3).i32Const(10).i32DivU().localSet(3)
            .br(0)
            .end().end()
            .localGet(5).localGet(2).i32Add().localGet(1).i32Sub(),
    });

    // alloc(size) -> ptr
    const alloc = module.addFunction({
        name: 'alloc',
        params: [VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32, VAL.i32],
        body: new Emitter()
            .globalGet(globals.heap).localSet(1)
            .globalGet(globals.heap).localGet(0).i32Add().i32Const(7).i32Add().i32Const(-8).i32And()
            .globalSet(globals.heap)
            .globalGet(globals.heap).i32Const(65535).i32Add().i32Const(16).i32ShrU().localSet(2)
            .localGet(2).memorySize().i32GtS().if()
            .localGet(2).memorySize().i32Sub().memoryGrow().drop()
            .end()
            .localGet(1),
    });

    // mkStr(dataPtr, len) -> header
    const mkStr = module.addFunction({
        name: 'mkStr',
        params: [VAL.i32, VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32],
        body: new Emitter()
            .i32Const(8).call(alloc).localSet(2)
            .localGet(2).localGet(0).i32Store(0)
            .localGet(2).localGet(1).i32Store(4)
            .localGet(2),
    });

    // intToStr(value) -> header
    const intToStr = module.addFunction({
        name: 'intToStr',
        params: [VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32, VAL.i32],
        body: new Emitter()
            .i32Const(12).call(alloc).localSet(1)
            .localGet(0).localGet(1).call(itoa).localSet(2)
            .localGet(1).localGet(2).call(mkStr),
    });

    // strCat(a, b) -> header
    const strCat = module.addFunction({
        name: 'strCat',
        params: [VAL.i32, VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32, VAL.i32, VAL.i32, VAL.i32, VAL.i32],
        body: new Emitter()
            .localGet(0).i32Load(0).localSet(2)
            .localGet(0).i32Load(4).localSet(3)
            .localGet(1).i32Load(0).localSet(4)
            .localGet(1).i32Load(4).localSet(5)
            .localGet(3).localGet(5).i32Add().call(alloc).localSet(6)
            .localGet(6).localGet(2).localGet(3).call(copy).drop()
            .localGet(6).localGet(3).i32Add().localGet(4).localGet(5).call(copy).drop()
            .localGet(6).localGet(3).localGet(5).i32Add().call(mkStr),
    });

    // jsonEscape(strHeader, dest) -> bytesWritten
    const jsonEscape = module.addFunction({
        name: 'jsonEscape',
        params: [VAL.i32, VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32, VAL.i32, VAL.i32, VAL.i32, VAL.i32, VAL.i32],
        body: new Emitter()
            .localGet(0).i32Load(0).localSet(2)
            .localGet(0).i32Load(4).localSet(3)
            .i32Const(0).localSet(4)
            .localGet(1).localSet(5)
            .block().loop()
            .localGet(4).localGet(3).i32GeS().brIf(1)
            .localGet(2).localGet(4).i32Add().i32Load8U().localSet(6)
            .i32Const(0).localSet(7)
            .localGet(6).i32Const(34).i32Eq().if().i32Const(34).localSet(7).end()
            .localGet(6).i32Const(92).i32Eq().if().i32Const(92).localSet(7).end()
            .localGet(6).i32Const(8).i32Eq().if().i32Const(98).localSet(7).end()
            .localGet(6).i32Const(9).i32Eq().if().i32Const(116).localSet(7).end()
            .localGet(6).i32Const(10).i32Eq().if().i32Const(110).localSet(7).end()
            .localGet(6).i32Const(12).i32Eq().if().i32Const(102).localSet(7).end()
            .localGet(6).i32Const(13).i32Eq().if().i32Const(114).localSet(7).end()
            .localGet(7).if()
            .localGet(5).i32Const(92).i32Store8()
            .localGet(5).i32Const(1).i32Add().localGet(7).i32Store8()
            .localGet(5).i32Const(2).i32Add().localSet(5)
            .else()
            .localGet(6).i32Const(32).i32LtS().if()
            .localGet(5).i32Const(32).i32Store8()
            .localGet(5).i32Const(1).i32Add().localSet(5)
            .else()
            .localGet(5).localGet(6).i32Store8()
            .localGet(5).i32Const(1).i32Add().localSet(5)
            .end()
            .end()
            .localGet(4).i32Const(1).i32Add().localSet(4)
            .br(0)
            .end().end()
            .localGet(5).localGet(1).i32Sub(),
    });

    // quoteStr(strHeader) -> header
    const quoteStr = module.addFunction({
        name: 'quoteStr',
        params: [VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32, VAL.i32, VAL.i32],
        body: new Emitter()
            .localGet(0).i32Load(4).localSet(1)
            .localGet(1).i32Const(2).i32Mul().i32Const(2).i32Add().call(alloc).localSet(2)
            .localGet(2).i32Const(34).i32Store8()
            .localGet(0).localGet(2).i32Const(1).i32Add().call(jsonEscape).localSet(3)
            .localGet(2).i32Const(1).i32Add().localGet(3).i32Add().i32Const(34).i32Store8()
            .localGet(2).localGet(3).i32Const(2).i32Add().call(mkStr),
    });

    // idiv(a, b) -> i32
    const idiv = module.addFunction({
        name: 'idiv',
        params: [VAL.i32, VAL.i32],
        results: [VAL.i32],
        body: new Emitter()
            .localGet(1).i32Eqz().if(VAL.i32)
            .i32Const(0)
            .else()
            .localGet(0).localGet(1).i32DivS()
            .end(),
    });

    // imod(a, b) -> i32
    const imod = module.addFunction({
        name: 'imod',
        params: [VAL.i32, VAL.i32],
        results: [VAL.i32],
        body: new Emitter()
            .localGet(1).i32Eqz().if(VAL.i32)
            .i32Const(0)
            .else()
            .localGet(0).localGet(1).i32RemS()
            .end(),
    });

    // dealloc(ptr, len)
    module.addFunction({ name: 'dealloc', params: [VAL.i32, VAL.i32], results: [], body: new Emitter() });

    // getHeap() -> i32
    module.addFunction({ name: 'getHeap', results: [VAL.i32], body: new Emitter().globalGet(globals.heap) });

    // setHeap(value)
    module.addFunction({ name: 'setHeap', params: [VAL.i32], results: [], body: new Emitter().localGet(0).globalSet(globals.heap) });

    return module.toBytes();
}

/**
 * Build a handler-only wasm module that imports the shared runtime.
 * The resulting module is much smaller than a monolithic build because it
 * does not embed the allocator, string utils, or JSON helpers.
 * @param {(context: WorkerRuntimeContext) => (Emitter | { body: Emitter, locals: number[] })} emitCall
 * @param {{ requestFields?: boolean }} [options]
 * @returns {WasmModule}
 */
export function buildHandlerModule(emitCall, options = {}) {
    const module = new WasmModule();

    // Import runtime memory
    module.addImportMemory({ module: 'env', name: 'memory', min: 2 });

    // Import runtime functions in a stable order
    const fns = {
        alloc: module.addImportFunction({ module: 'env', name: 'alloc', params: [VAL.i32], results: [VAL.i32] }),
        dealloc: module.addImportFunction({ module: 'env', name: 'dealloc', params: [VAL.i32, VAL.i32], results: [] }),
        copy: module.addImportFunction({ module: 'env', name: 'copy', params: [VAL.i32, VAL.i32, VAL.i32], results: [VAL.i32] }),
        itoa: module.addImportFunction({ module: 'env', name: 'itoa', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
        mkStr: module.addImportFunction({ module: 'env', name: 'mkStr', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
        intToStr: module.addImportFunction({ module: 'env', name: 'intToStr', params: [VAL.i32], results: [VAL.i32] }),
        strCat: module.addImportFunction({ module: 'env', name: 'strCat', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
        jsonEscape: module.addImportFunction({ module: 'env', name: 'jsonEscape', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
        quoteStr: module.addImportFunction({ module: 'env', name: 'quoteStr', params: [VAL.i32], results: [VAL.i32] }),
        idiv: module.addImportFunction({ module: 'env', name: 'idiv', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
        imod: module.addImportFunction({ module: 'env', name: 'imod', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
        setHeap: module.addImportFunction({ module: 'env', name: 'setHeap', params: [VAL.i32], results: [] }),
    };

    // Host-provided request-field accessors
    const imports = options.requestFields
        ? {
            query: module.addImportFunction({ module: 'env', name: 'req_query', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
            path: module.addImportFunction({ module: 'env', name: 'req_path', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
            header: module.addImportFunction({ module: 'env', name: 'req_header', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
            platform: module.addImportFunction({ module: 'env', name: 'req_platform', params: [VAL.i32, VAL.i32], results: [VAL.i32] }),
        }
        : null;

    // Handler-specific globals
    const globals = {
        outPtr: module.addGlobal({ type: VAL.i32, mutable: true, init: 0 }),
        outLen: module.addGlobal({ type: VAL.i32, mutable: true, init: 0 }),
        bodyPtr: module.addGlobal({ type: VAL.i32, mutable: true, init: 0 }),
        bodyLen: module.addGlobal({ type: VAL.i32, mutable: true, init: 0 }),
    };

    // Intern string constants into a single data segment placed at DATA_BASE.
    /** @type {number[]} */
    const dataBytes = [];
    /** @param {string} text */
    const intern = (text) => {
        const bytes = Array.from(new TextEncoder().encode(text));
        const offset = DATA_BASE + dataBytes.length;
        dataBytes.push(...bytes);
        return { offset, length: bytes.length };
    };

    const callResult = emitCall({ module, globals, fns, imports, intern });
    const callBody = callResult instanceof Emitter ? callResult : callResult.body;
    const callLocals = callResult instanceof Emitter ? [VAL.i32, VAL.i32, VAL.i32] : callResult.locals;

    // Reset the bump allocator via the imported setHeap at the end of every call.
    const callWithReset = new Emitter()
        .raw(callBody.bytes)
        .i32Const(HEAP_BASE).call(fns.setHeap);

    if (DATA_BASE + dataBytes.length > HEAP_BASE)
        throw new Error(`Tac worker constant data (${dataBytes.length} bytes) overflows the heap base (${HEAP_BASE})`);
    if (dataBytes.length > 0)
        module.addData(DATA_BASE, dataBytes);

    module.addFunction({
        name: 'call',
        params: [VAL.i32, VAL.i32, VAL.i32, VAL.i32, VAL.i32, VAL.i32],
        results: [],
        locals: callLocals,
        body: callWithReset,
    });

    module.addFunction({ name: 'output_ptr', results: [VAL.i32], body: new Emitter().globalGet(globals.outPtr) });
    module.addFunction({ name: 'output_len', results: [VAL.i32], body: new Emitter().globalGet(globals.outLen) });

    return module;
}

/**
 * Compile a baseline Tac Worker that echoes the request byte length back as a
 * JSON response. This proves the in-house pipeline produces worker-compatible
 * wasm; richer per-handler codegen lowers into the same `call` shape.
 * @param {{ engine?: string, message?: string }} [options]
 * @returns {Uint8Array}
 */
export function compileEchoWorker(options = {}) {
    const engine = options.engine ?? 'tac-wasm';
    const message = options.message ?? 'Compiled in-house to WebAssembly with no external toolchain';
    const prefixText = `{"status":200,"headers":{"Content-Type":"application/json"},"body":{"engine":"${engine}","requestBytes":`;
    const suffixText = `,"message":"${message}"}}`;

    const module = buildWorkerModule(({ globals, fns, intern }) => {
        const prefix = intern(prefixText);
        const suffix = intern(suffixText);
        // params: methodPtr=0, methodLen=1, requestPtr=2, requestLen=3, bodyPtr=4, bodyLen=5
        // locals: out=6, pos=7, n=8
        return new Emitter()
            .i32Const(256).call(fns.alloc).localSet(6)
            .localGet(6).localSet(7)
            .localGet(7).i32Const(prefix.offset).i32Const(prefix.length).call(fns.copy).localSet(8)
            .localGet(7).localGet(8).i32Add().localSet(7)
            .localGet(3).localGet(7).call(fns.itoa).localSet(8)
            .localGet(7).localGet(8).i32Add().localSet(7)
            .localGet(7).i32Const(suffix.offset).i32Const(suffix.length).call(fns.copy).localSet(8)
            .localGet(7).localGet(8).i32Add().localSet(7)
            .localGet(6).globalSet(globals.outPtr)
            .localGet(7).localGet(6).i32Sub().globalSet(globals.outLen);
    });

    return module.toBytes();
}
