// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import {
    clearToolchainConfig,
    loadToolchainConfig,
    resolveInterpreter,
} from '../../src/shared/toolchain-config.js';

/** @type {string[]} */
const tempDirs = [];

afterEach(async () => {
    clearToolchainConfig();
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

test('default interpreters map common extensions to run commands', () => {
    clearToolchainConfig();
    const config = loadToolchainConfig(path.join(tmpdir(), 'no-such-project'));
    expect(resolveInterpreter('.go', config)).toEqual(['go', 'run']);
    expect(resolveInterpreter('py', config)).toEqual(['python3']); // dot optional
    expect(resolveInterpreter('.LUA', config)).toEqual(['lua']);   // case-insensitive
    expect(resolveInterpreter('.nope', config)).toBeNull();        // unknown → null
});

test('.tachyonrc adds and overrides interpreters (string or array)', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-interp-'));
    tempDirs.push(root);
    await writeFile(path.join(root, '.tachyonrc'), JSON.stringify({
        interpreters: {
            '.myl': 'my-lang run',          // brand-new extension, string form
            go: ['go', 'run', '-race'],     // override a default, array form, no dot
        },
    }));
    clearToolchainConfig();
    const config = loadToolchainConfig(root);
    expect(resolveInterpreter('.myl', config)).toEqual(['my-lang', 'run']);
    expect(resolveInterpreter('.go', config)).toEqual(['go', 'run', '-race']);
    // Untouched defaults survive the merge.
    expect(resolveInterpreter('.rb', config)).toEqual(['ruby']);
});
