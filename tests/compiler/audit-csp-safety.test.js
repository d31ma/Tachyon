// @ts-check
import { afterEach, describe, expect, test } from 'bun:test';
import { mkdtemp, rm, writeFile, mkdir } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import Compiler from '../../src/compiler/index.js';

/** @type {string[]} */
const roots = [];
afterEach(async () => {
    await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

/** @param {Record<string, string>} files */
async function fixture(files) {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-csp-'));
    roots.push(root);
    for (const [rel, source] of Object.entries(files)) {
        const full = path.join(root, rel);
        await mkdir(path.dirname(full), { recursive: true });
        await writeFile(full, source);
    }
    return root;
}

describe('Compiler.auditCspSafety', () => {
    test('reports each unsafe-eval construct with a count and relative path', async () => {
        const root = await fixture({
            'pages/tac.js': 'const f = new Function("return 1"); eval("2");',
            'components/x/tac.js': 'const AsyncFunction = Object.getPrototypeOf(async()=>{}).constructor; new AsyncFunction("");',
        });
        const findings = await Compiler.auditCspSafety(root);
        expect(findings).toContainEqual({ file: 'pages/tac.js', construct: 'eval()', count: 1 });
        expect(findings).toContainEqual({ file: 'pages/tac.js', construct: 'new Function()', count: 1 });
        expect(findings.some((f) => f.file === 'components/x/tac.js' && f.construct === 'AsyncFunction constructor')).toBe(true);
    });

    test('passes clean output and ignores non-JS files', async () => {
        const root = await fixture({
            'pages/tac.js': 'const scope = {}; scope.status = "ready"; export default () => scope.status;',
            'data.json': '{"note": "this eval( and new Function( are just text"}',
            'style.css': '.x { content: "eval("; }',
        });
        expect(await Compiler.auditCspSafety(root)).toEqual([]);
    });

    test('does not flag identifiers that merely contain the words', async () => {
        const root = await fixture({
            'pages/tac.js': 'const evaluate = 1; const myFunction = 2; export default () => evaluate + myFunction;',
        });
        expect(await Compiler.auditCspSafety(root)).toEqual([]);
    });
});
