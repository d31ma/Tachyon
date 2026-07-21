// @ts-check
import { afterAll, expect, test } from 'bun:test';
import { mkdir, mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import { quickJSDriverHeader, quickJSDriverSource } from '../../src/compiler/native-ui/quickjs-driver.js';

/** @type {string[]} */
const tempDirs = [];
afterAll(async () => Promise.all(tempDirs.splice(0).map((directory) => rm(directory, { recursive: true, force: true }))));

test('desktop controller driver owns QuickJS values and pumps promise jobs', async () => {
    const source = quickJSDriverSource();
    expect(source).toContain('JS_ExecutePendingJob');
    expect(source).toContain('JS_PromiseState');
    expect(source).toContain('tachyon_ui_controller_dispatch');
    expect(source).toContain('__tachyonNativeHostCall');
    expect(source).toContain('tachyon_ui_controller_emit');
    expect(quickJSDriverHeader()).toContain('TachyonUIController');
    expect(quickJSDriverHeader()).toContain('TachyonNativeHostHandler');

    const quickJSRoot = '/tmp/quickjs-0.15.1';
    const compiler = Bun.which('cc');
    if (!await Bun.file(path.join(quickJSRoot, 'quickjs.h')).exists() || !compiler) return;
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-quickjs-driver-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'src'));
    await writeFile(path.join(root, 'src', 'tachyon_ui_controller.h'), quickJSDriverHeader());
    await writeFile(path.join(root, 'src', 'tachyon_ui_controller.c'), source);
    const build = Bun.spawn([
        compiler, '-std=c11', '-fsyntax-only', `-I${quickJSRoot}`, `-I${path.join(root, 'src')}`,
        path.join(root, 'src', 'tachyon_ui_controller.c'),
    ], { stdout: 'pipe', stderr: 'pipe' });
    const [buildError, buildCode] = await Promise.all([new Response(build.stderr).text(), build.exited]);
    if (buildCode !== 0) throw new Error(buildError);
});
