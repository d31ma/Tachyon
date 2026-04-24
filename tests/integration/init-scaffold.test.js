// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { mkdtemp, mkdir, readFile, rm } from 'fs/promises';
import path from 'path';
import { tmpdir } from 'os';
import { createAppScaffold } from '../../src/runtime/app-scaffold.js';
/** @type {string[]} */
const tempDirs = [];
afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});
test('createAppScaffold writes a deployable starter app', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-init-'));
    const appDir = path.join(root, 'my-app');
    tempDirs.push(root);
    const created = await createAppScaffold(appDir);
    expect(created).toBe(appDir);
    const packageJson = await readFile(path.join(appDir, 'package.json'), 'utf8');
    const homePage = await readFile(path.join(appDir, 'browser', 'pages', 'index.html'), 'utf8');
    const mainScript = await readFile(path.join(appDir, 'browser', 'shared', 'scripts', 'main.js'), 'utf8');
    const sharedStyle = await readFile(path.join(appDir, 'browser', 'shared', 'styles', 'app.css'), 'utf8');
    const handler = await readFile(path.join(appDir, 'server', 'routes', 'GET'), 'utf8');
    expect(packageJson).toContain('"serve": "yon.serve"');
    expect(packageJson).toContain('"preview": "tac.preview --watch"');
    expect(packageJson).toContain('"@d31ma/tachyon"');
    expect(homePage).toContain('<hero />');
    expect(homePage).toContain('<slot />');
    expect(mainScript).toContain('../styles/app.css');
    expect(sharedStyle).toContain('color-scheme: dark');
    expect(handler).toContain(`framework: 'Tachyon'`);
    expect(await Bun.file(path.join(appDir, 'amplify.yml')).exists()).toBe(false);
});
test('createAppScaffold refuses to overwrite a non-empty directory', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-init-'));
    const appDir = path.join(root, 'existing');
    tempDirs.push(root);
    await mkdir(appDir, { recursive: true });
    await Bun.write(path.join(appDir, 'README.md'), 'existing');
    await expect(createAppScaffold(appDir)).rejects.toThrow('is not empty');
});
