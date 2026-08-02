#!/usr/bin/env node
// Proves the wasm companion ABI of ADR 0011 against a module compiled by bare
// rustc: three exports, one JSON entry point, no bindgen and no glue crate.
//
// This is the contract check, not the compiler pipeline. It answers whether a
// language that can target wasm can present a Tac companion at all.

import { readFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

// The host proxy from ADR 0011: property reads become get, assignments become
// set, method calls become call. Nothing above this knows it is wasm.
const wasmCompanion = async (bytes, props = {}) => {
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const { memory, tac_alloc, tac_invoke } = instance.exports;
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const invoke = (request, tolerant = false) => {
    const payload = encoder.encode(JSON.stringify(request));
    const ptr = tac_alloc(payload.length);
    new Uint8Array(memory.buffer, ptr, payload.length).set(payload);
    const packed = tac_invoke(ptr, payload.length);
    const outPtr = Number(packed >> 32n);
    const outLen = Number(packed & 0xffffffffn);
    const text = decoder.decode(new Uint8Array(memory.buffer, outPtr, outLen));
    const response = JSON.parse(text);
    if (response.error) {
      if (tolerant) return undefined;
      throw new Error(response.error);
    }
    return response.value;
  };
  // init declares the members, so the host never guesses whether a name is a
  // field or a method, and an unknown name is undefined as on a plain object.
  const members = invoke({ op: 'init', props }) ?? {};
  const fields = new Set(members.fields ?? []);
  const methods = new Set(members.methods ?? []);
  return new Proxy({}, {
    get: (_, name) => {
      if (typeof name !== 'string') return undefined;
      if (fields.has(name)) return invoke({ op: 'get', name });
      if (methods.has(name)) return (...args) => invoke({ op: 'call', name, args });
      return undefined;
    },
    set: (_, name, value) => { invoke({ op: 'set', name, value }); return true; },
  });
};

const here = path.dirname(fileURLToPath(import.meta.url));
const fixture = path.join(here, 'abi-fixture.rs');
const wasm = path.join(tmpdir(), 'tachyon-abi-fixture.wasm');
const built = spawnSync(
  'rustc',
  [
    '--target', 'wasm32-unknown-unknown',
    '--crate-type', 'cdylib',
    // The same flags the compiler uses, so the probe cannot pass against a
    // configuration the build does not use.
    '--edition', '2021',
    '-O',
    // Symbol names and metadata, not code, are what make a wasm module large:
    // this fixture is 819 KB without stripping and 21 KB with it.
    '-C', 'strip=symbols',
    '-o', wasm, fixture,
  ],
  { encoding: 'utf8' },
);
if (built.status !== 0) {
  console.error(built.stderr || built.stdout);
  console.error('rustc cannot target wasm here; run: ty doctor');
  process.exit(1);
}

const bytes = await readFile(wasm);
const instance = await wasmCompanion(bytes);
let failed = 0;
const expect = (actual, wanted, label) => {
  if (actual === wanted) console.log(`    ok   ${label}: ${actual}`);
  else { failed += 1; console.error(`  FAIL   ${label}: expected ${wanted}, got ${actual}`); }
};

expect(instance.count, 6, 'field read');
expect(instance.label, 'from rust', 'string field read');
expect(instance.doubled(), 12, 'method call');
instance.count = 20;
expect(instance.count, 20, 'field write');
expect(instance.doubled(), 40, 'method sees the written field');

// Reading a member the module does not expose yields undefined, as a plain
// object does, which is what lets the proxy survive being awaited.
expect(instance.missingMember, undefined, 'unknown member is undefined');
expect(typeof instance.then, 'undefined', 'awaiting the proxy does not probe the module');

// A module that ships per component has to stay small, so the size is a
// checked property rather than an observation made once.
const size = (await readFile(wasm)).length;
expect(size < 64 * 1024, true, `module is ${size} bytes, under the 64 KiB budget`);

if (failed) process.exit(1);
console.log('\nwasm companion ABI probe passed');
