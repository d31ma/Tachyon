// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { access, mkdtemp, readFile, rm, unlink } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import TachyonRuntimeCache from '../../src/shared/runtime-cache.js';

/** @type {string[]} */
const roots = [];
const previousCacheDir = process.env.TACHYON_CACHE_DIR;

afterEach(async () => {
    if (previousCacheDir === undefined)
        delete process.env.TACHYON_CACHE_DIR;
    else
        process.env.TACHYON_CACHE_DIR = previousCacheDir;
    for (const root of roots.splice(0))
        await rm(root, { recursive: true, force: true });
});

/** @param {string} filePath */
async function exists(filePath) {
    try {
        await access(filePath);
        return true;
    }
    catch {
        return false;
    }
}

test('materializes, validates, repairs, and clears the standalone runtime cache', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-runtime-cache-'));
    roots.push(root);
    process.env.TACHYON_CACHE_DIR = root;
    const files = [
        { path: 'runtime/example.js', source: 'export const answer = 42;\n' },
        { path: 'runtime/nested/helper.js', source: 'export const helper = true;\n' },
    ];

    const [cachedRoot, concurrentRoot] = await Promise.all([
        TachyonRuntimeCache.materialize(files),
        TachyonRuntimeCache.materialize(files),
    ]);
    expect(concurrentRoot).toBe(cachedRoot);
    expect(cachedRoot.startsWith(path.join(root, 'runtime'))).toBe(true);
    expect(await readFile(path.join(cachedRoot, 'runtime/example.js'), 'utf8')).toBe(files[0].source);
    expect(JSON.parse(await readFile(path.join(cachedRoot, 'manifest.json'), 'utf8')).files).toHaveLength(2);

    await unlink(path.join(cachedRoot, 'runtime/example.js'));
    const repairedRoot = await TachyonRuntimeCache.materialize(files);
    expect(repairedRoot).toBe(cachedRoot);
    expect(await readFile(path.join(repairedRoot, 'runtime/example.js'), 'utf8')).toBe(files[0].source);

    await TachyonRuntimeCache.clear();
    expect(await exists(path.join(root, 'runtime'))).toBe(false);
});
