// @ts-check
import { afterEach, expect, mock, test } from 'bun:test';
import { installFakeOpfs } from './fake-opfs.js';
import * as opfsFs from '../../src/runtime/fylo-opfs-fs.js';
import { __resetOpfsFs } from '../../src/runtime/fylo-opfs-fs.js';

const previousNavigator = globalThis.navigator;

afterEach(() => {
    __resetOpfsFs();
    Object.defineProperty(globalThis, 'navigator', { configurable: true, writable: true, value: previousNavigator });
});

// SKIPPED: the OPFS VFS handles FYLO's node:fs/promises *writes* (FYLO produces
// its exact on-disk format here), but FYLO also reads/hashes/IDs via Bun-native
// APIs (Bun.file, Bun.CryptoHasher, Bun.mmap, Bun.randomUUIDv7, Bun.JSONL).
// Running unmodified FYLO in a browser needs a Bun-runtime shim too — pending a
// decision on whether to shim Bun in Tachyon or abstract FYLO's runtime.
test.skip('real @d31ma/fylo persists and queries through the OPFS VFS', async () => {
    // Alias node:fs/promises -> the OPFS VFS, so the real FYLO engine's storage
    // lands in OPFS. (In Bun, node:path / Buffer / crypto / node:url are native,
    // so only the filesystem needs aliasing — exactly what the browser worker
    // does.) Kept *inside* the skipped test: at module scope it executes at load
    // time and globally breaks every other suite's `fs/promises` import.
    mock.module('node:fs/promises', () => ({ ...opfsFs }));

    const root = installFakeOpfs();
    const { default: Fylo } = await import('@d31ma/fylo');

    const fylo = new Fylo('fylo-db', { index: { backend: 'local-fs' } });
    await fylo.createCollection('users');

    const id = await fylo.putData('users', { name: 'Ada', role: 'admin' });
    expect(typeof id).toBe('string');

    /** @type {unknown[]} */
    const docs = [];
    for await (const entry of fylo.findDocs('users', {}).collect())
        docs.push(entry);

    // The document round-trips through the *real* engine on the OPFS VFS.
    expect(JSON.stringify(docs)).toContain('Ada');

    // Proof of "no deviation": FYLO wrote its own on-disk layout into OPFS.
    expect([...root.dirs.keys()]).toContain('fylo-db');
});
