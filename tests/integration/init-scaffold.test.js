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
    const created = await createAppScaffold(appDir, { appName: 'Acme Desk' });
    expect(created).toBe(appDir);
    const packageJson = await readFile(path.join(appDir, 'package.json'), 'utf8');
    const homePage = await readFile(path.join(appDir, 'client', 'pages', 'tac.html'), 'utf8');
    const mainScript = await readFile(path.join(appDir, 'client', 'shared', 'scripts', 'imports.js'), 'utf8');
    const sharedStyle = await readFile(path.join(appDir, 'client', 'shared', 'styles', 'app.css'), 'utf8');
    const handler = await readFile(path.join(appDir, 'server', 'routes', 'yon.js'), 'utf8');
    const jsconfig = await readFile(path.join(appDir, 'jsconfig.json'), 'utf8');
    const tachyonEnv = await readFile(path.join(appDir, 'tachyon-env.d.ts'), 'utf8');
    expect(packageJson).toContain('"serve": "ty serve"');
    expect(packageJson).toContain('"name": "acme-desk"');
    expect(packageJson).toContain('"preview": "ty preview --watch"');
    // Off npm: no @d31ma/tachyon dependency; the CLI is the `ty` binary.
    expect(packageJson).not.toContain('@d31ma/tachyon');
    expect(jsconfig).toContain('tachyon-env.d.ts');
    // Globals are embedded self-contained (no @d31ma/tachyon/globals reference).
    expect(tachyonEnv).toContain('const fylo: FyloApi');
    expect(tachyonEnv).toContain('invoke<T = unknown>(operation: string');
    expect(tachyonEnv).toContain('const shortcuts:');
    expect(tachyonEnv).toContain('const contentSurface:');
    expect(tachyonEnv).toContain('const screenCapture:');
    expect(tachyonEnv).not.toContain('@d31ma/tachyon/globals');
    expect(await readFile(path.join(appDir, 'client', 'pages', 'tac.js'), 'utf8')).toContain('document.title = "Acme Desk"');
    expect(await Bun.file(path.join(appDir, 'browser')).exists()).toBe(false);
    expect(await Bun.file(path.join(appDir, '.env.test')).exists()).toBe(true);
    expect(await Bun.file(path.join(appDir, '.env.example')).exists()).toBe(true);
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
