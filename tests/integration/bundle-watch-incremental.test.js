// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'fs/promises';
import path from 'path';
import { tmpdir } from 'os';
const timedTest = /** @type {any} */ (test);
const bundleEntrypoint = path.join(import.meta.dir, '../../src/cli/bundle.js');
/** @type {string[]} */
const tempDirs = [];
/** @type {Bun.Subprocess<any, any, any>[]} */
const processes = [];
afterEach(async () => {
    for (const proc of processes.splice(0)) {
        proc.kill();
        await proc.exited.catch(() => { });
    }
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});
/** @param {() => boolean | Promise<boolean>} check */
async function waitFor(check, timeoutMs = 15000, intervalMs = 150) {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
        if (await check())
            return;
        await Bun.sleep(intervalMs);
    }
    throw new Error(`Condition not met within ${timeoutMs}ms`);
}
async function createFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-watch-inc-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages', 'docs'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({ name: 'fixture', private: true }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), '<main><h1>Alpha</h1></main>');
    await writeFile(path.join(root, 'client', 'pages', 'docs', 'tac.html'), '<main><h1>Docs</h1></main>');
    return root;
}
timedTest('watch rebuilds a page incrementally without re-rendering siblings', { timeout: 25000 }, async () => {
    const root = await createFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint, '--watch'], {
        cwd: root,
        env: { ...process.env, YON_LOG_FORMAT: 'json' },
        stdout: 'pipe',
        stderr: 'pipe'
    });
    processes.push(proc);
    let stdout = '';
    (async () => {
        const reader = /** @type {ReadableStream<Uint8Array>} */ (proc.stdout).getReader();
        const decoder = new TextDecoder();
        for (;;) {
            const { done, value } = await reader.read();
            if (done)
                break;
            stdout += decoder.decode(value, { stream: true });
        }
    })();
    const homeIndex = path.join(root, 'dist', 'web', 'index.html');
    const docsIndex = path.join(root, 'dist', 'web', 'docs', 'index.html');
    // Initial full build lands both routes.
    await waitFor(async () => stdout.includes('Bundle completed') && (await readFile(homeIndex, 'utf8')).includes('Alpha'));
    const docsMtimeBefore = (await stat(docsIndex)).mtimeMs;
    // Edit one page; the watcher should take the incremental path.
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), '<main><h1>Beta</h1></main>');
    await waitFor(async () => stdout.includes('(incremental)') && (await readFile(homeIndex, 'utf8')).includes('Beta'));
    // The edited route reflects the change; the untouched sibling was not re-rendered.
    expect(await readFile(homeIndex, 'utf8')).toContain('Beta');
    expect((await stat(docsIndex)).mtimeMs).toBe(docsMtimeBefore);
});
timedTest('watch rebuilds a component module incrementally', { timeout: 25000 }, async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-watch-comp-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'badge'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({ name: 'fixture', private: true }, null, 2));
    await writeFile(path.join(root, 'client', 'components', 'badge', 'tac.html'), '<span>One</span>');
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), '<main><badge></badge></main>');
    const proc = Bun.spawn(['bun', bundleEntrypoint, '--watch'], {
        cwd: root,
        env: { ...process.env, YON_LOG_FORMAT: 'json' },
        stdout: 'pipe',
        stderr: 'pipe'
    });
    processes.push(proc);
    let stdout = '';
    (async () => {
        const reader = /** @type {ReadableStream<Uint8Array>} */ (proc.stdout).getReader();
        const decoder = new TextDecoder();
        for (;;) {
            const { done, value } = await reader.read();
            if (done)
                break;
            stdout += decoder.decode(value, { stream: true });
        }
    })();
    // Components hydrate client-side from their own module, so the edit lands in
    // the component bundle, not the prerendered page HTML.
    const componentModule = path.join(root, 'dist', 'web', 'components', 'badge', 'tac.js');
    await waitFor(async () => stdout.includes('Bundle completed') && (await readFile(componentModule, 'utf8')).includes('One'));
    await writeFile(path.join(root, 'client', 'components', 'badge', 'tac.html'), '<span>Two</span>');
    await waitFor(async () => stdout.includes('(incremental)') && (await readFile(componentModule, 'utf8')).includes('Two'));
    expect(await readFile(componentModule, 'utf8')).toContain('Two');
});
