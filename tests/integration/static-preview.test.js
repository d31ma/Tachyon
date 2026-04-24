// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { mkdtemp, mkdir, rm, writeFile } from 'fs/promises';
import path from 'path';
import { tmpdir } from 'os';
import { createStaticPreviewServer } from '../../src/runtime/static-preview.js';
/** @type {string[]} */
const tempDirs = [];
afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});
async function createDistFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-preview-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'dist', 'docs'), { recursive: true });
    await mkdir(path.join(root, 'dist', 'shared', 'assets'), { recursive: true });
    await writeFile(path.join(root, 'dist', 'index.html'), '<!DOCTYPE html><html><body><h1>Home</h1></body></html>');
    await writeFile(path.join(root, 'dist', 'docs', 'index.html'), '<!DOCTYPE html><html><body><h1>Docs</h1></body></html>');
    await writeFile(path.join(root, 'dist', 'main.js'), 'console.log("tachyon");');
    await writeFile(path.join(root, 'dist', 'shared', 'assets', 'logo.svg'), '<svg xmlns="http://www.w3.org/2000/svg"></svg>');
    return path.join(root, 'dist');
}
async function createDistFixtureWithoutRoot() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-preview-missing-root-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'dist', 'docs'), { recursive: true });
    await writeFile(path.join(root, 'dist', 'docs', 'index.html'), '<!DOCTYPE html><html><body><h1>Docs</h1></body></html>');
    return path.join(root, 'dist');
}
test('static preview serves nested route indexes and bundle assets', async () => {
    const distPath = await createDistFixture();
    const server = await createStaticPreviewServer(distPath, { port: 0, hostname: '127.0.0.1' });
    try {
        const base = `http://${server.hostname}:${server.port}`;
        const [home, docs, js, asset, headHome, missingAsset] = await Promise.all([
            fetch(`${base}/`),
            fetch(`${base}/docs`),
            fetch(`${base}/main.js`),
            fetch(`${base}/shared/assets/logo.svg`),
            fetch(`${base}/`, { method: 'HEAD' }),
            fetch(`${base}/missing.js`)
        ]);
        expect(home.status).toBe(200);
        expect(await home.text()).toContain('<h1>Home</h1>');
        expect(home.headers.get('cache-control')).toBe('no-cache, must-revalidate');
        expect(docs.status).toBe(200);
        expect(await docs.text()).toContain('<h1>Docs</h1>');
        expect(docs.headers.get('cache-control')).toBe('no-cache, must-revalidate');
        expect(js.status).toBe(200);
        expect(await js.text()).toContain('console.log("tachyon")');
        expect(js.headers.get('cache-control')).toBe('no-cache, must-revalidate');
        expect(asset.status).toBe(200);
        expect((await asset.text()).trim()).toBe('<svg xmlns="http://www.w3.org/2000/svg"></svg>');
        expect(asset.headers.get('cache-control')).toBe('public, max-age=3600');
        expect(headHome.status).toBe(200);
        expect(await headHome.text()).toBe('');
        expect(missingAsset.status).toBe(404);
    }
    finally {
        server.stop();
    }
});
test('static preview explains missing root entry points', async () => {
    const distPath = await createDistFixtureWithoutRoot();
    const server = await createStaticPreviewServer(distPath, { port: 0, hostname: '127.0.0.1' });
    try {
        const base = `http://${server.hostname}:${server.port}`;
        const [root, docs] = await Promise.all([
            fetch(`${base}/`),
            fetch(`${base}/docs`)
        ]);
        expect(root.status).toBe(404);
        expect(await root.text()).toContain('No previewable file was found for "/"');
        expect(docs.status).toBe(200);
        expect(await docs.text()).toContain('<h1>Docs</h1>');
    }
    finally {
        server.stop();
    }
});
