// @ts-check
import { expect, test } from 'bun:test';
import { Emitter, VAL, WasmModule, signedLEB, unsignedLEB } from '../../src/compiler/wasm/wasm-module.js';

test('unsignedLEB encodes canonical examples', () => {
    expect(unsignedLEB(0)).toEqual([0x00]);
    expect(unsignedLEB(127)).toEqual([0x7f]);
    expect(unsignedLEB(128)).toEqual([0x80, 0x01]);
    expect(unsignedLEB(624485)).toEqual([0xe5, 0x8e, 0x26]);
});

test('signedLEB encodes canonical examples (incl. negatives)', () => {
    expect(signedLEB(0)).toEqual([0x00]);
    expect(signedLEB(-1)).toEqual([0x7f]);
    expect(signedLEB(63)).toEqual([0x3f]);
    expect(signedLEB(64)).toEqual([0xc0, 0x00]);
    expect(signedLEB(-64)).toEqual([0x40]);
    expect(signedLEB(-65)).toEqual([0xbf, 0x7f]);
    expect(signedLEB(-123456)).toEqual([0xc0, 0xbb, 0x78]);
});

test('emits a valid module that instantiates and computes', async () => {
    const module = new WasmModule();
    module.setMemory(1);

    // add(a, b) = a + b
    module.addFunction({
        name: 'add',
        params: [VAL.i32, VAL.i32],
        results: [VAL.i32],
        body: new Emitter().localGet(0).localGet(1).i32Add(),
    });

    // sum_to(n) = 1 + 2 + ... + n  (exercises locals + block/loop/br_if/br)
    module.addFunction({
        name: 'sum_to',
        params: [VAL.i32],
        results: [VAL.i32],
        locals: [VAL.i32, VAL.i32], // acc = local 1, i = local 2
        body: new Emitter()
            .i32Const(0).localSet(1)
            .i32Const(1).localSet(2)
            .block()
            .loop()
            .localGet(2).localGet(0).i32GtS().brIf(1) // if i > n: break
            .localGet(1).localGet(2).i32Add().localSet(1) // acc += i
            .localGet(2).i32Const(1).i32Add().localSet(2) // i += 1
            .br(0)
            .end()
            .end()
            .localGet(1),
    });

    // first_byte() reads byte 0 of a data segment ("AB" -> 65)
    module.addData(0, [65, 66]);
    module.addFunction({
        name: 'first_byte',
        results: [VAL.i32],
        body: new Emitter().i32Const(0).i32Load8U(),
    });

    const bytes = module.toBytes();
    // Magic + version sanity.
    expect(Array.from(bytes.slice(0, 8))).toEqual([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);

    // The engine validates the binary on compile; this throws if encoding is wrong.
    const { instance } = await WebAssembly.instantiate(bytes, {});
    const exports = /** @type {Record<string, CallableFunction>} */ (/** @type {unknown} */ (instance.exports));

    expect(exports.add(2, 3)).toBe(5);
    expect(exports.add(-4, 9)).toBe(5);
    expect(exports.sum_to(5)).toBe(15);
    expect(exports.sum_to(100)).toBe(5050);
    expect(exports.sum_to(0)).toBe(0);
    expect(exports.first_byte()).toBe(65);
    expect(instance.exports.memory).toBeInstanceOf(WebAssembly.Memory);
});
