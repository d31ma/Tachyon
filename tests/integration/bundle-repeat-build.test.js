// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { access, mkdtemp, mkdir, readFile, rm, unlink, writeFile } from 'fs/promises';
import path from 'path';
import { tmpdir } from 'os';
const timedTest = /** @type {any} */ (test);
/** @type {string[]} */
const tempDirs = [];
const bundleEntrypoint = path.join(process.cwd(), 'src/cli/bundle.js');
/** @param {ReadableStream<Uint8Array> | null | undefined} stream */
async function decode(stream) {
    if (!stream)
        return '';
    return await new Response(stream).text();
}
/** @param {string} cwd */
async function runBundle(cwd) {
    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe'
    });
    const [stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ]);
    expect(exitCode).toBe(0);
    expect(stderr).toBe('');
    expect(stdout).toContain('Bundle completed');
}
async function createFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-repeat-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes', 'docs'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({ name: 'fixture', private: true }, null, 2));
    await writeFile(path.join(root, 'routes', 'index.html'), '<h1>Home</h1>');
    await writeFile(path.join(root, 'routes', 'docs', 'index.html'), '<h1>Docs</h1>');
    return root;
}
afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});
timedTest('successive bundle runs remove stale route output', { timeout: 20000 }, async () => {
    const cwd = await createFixture();
    await runBundle(cwd);
    const docsIndex = path.join(cwd, 'dist', 'docs', 'index.html');
    expect(await readFile(docsIndex, 'utf8')).toContain('Docs');
    await unlink(path.join(cwd, 'routes', 'docs', 'index.html'));
    await runBundle(cwd);
    await expect(access(docsIndex)).rejects.toBeDefined();
});
