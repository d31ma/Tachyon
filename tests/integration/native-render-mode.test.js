// @ts-check
import { afterAll, expect, test } from 'bun:test';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';

const bundleEntrypoint = path.join(process.cwd(), 'src', 'cli', 'bundle.js');
/** @type {string[]} */
const tempDirs = [];

afterAll(async () => {
    await Promise.all(tempDirs.splice(0).map((directory) => rm(directory, { recursive: true, force: true })));
});

/** @param {string} cwd @param {string[]} args */
async function runBundle(cwd, args) {
    const processHandle = Bun.spawn(['bun', bundleEntrypoint, ...args], {
        cwd,
        env: { ...process.env, NODE_ENV: 'test' },
        stdout: 'pipe',
        stderr: 'pipe',
    });
    const [stdout, stderr, exitCode] = await Promise.all([
        new Response(processHandle.stdout).text(),
        new Response(processHandle.stderr).text(),
        processHandle.exited,
    ]);
    return { stdout, stderr, exitCode };
}

async function createFixture(page) {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-native-render-mode-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'product', 'card'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({ name: 'native-render-mode-fixture', private: true }));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), page);
    await writeFile(path.join(root, 'client', 'components', 'product', 'card', 'tac.html'),
        '<article class="product"><h2>{name}</h2><button>Select</button></article>');
    return root;
}

test('non-web bundles default to native UI and lower control flow plus Tac components before writing the route document', async () => {
    const root = await createFixture(`
<script>
const products = [{ id: 'one', name: 'One', available: true }, { id: 'two', name: 'Two', available: false }]
</script>
<main>
  <loop :for="product of products">
    <logic :if="product.available">
      <product-card :key="product.id" :id="product.id" :name="product.name" />
    </logic>
    <logic :if="!product.available"><p>{product.name} unavailable</p></logic>
  </loop>
</main>`);

    const result = await runBundle(root, ['--target', 'macos', '--skip-native-host']);
    if (result.exitCode !== 0)
        throw new Error(result.stderr);
    expect(result.exitCode).toBe(0);
    const nativeBundle = JSON.parse(await readFile(path.join(root, 'dist', 'macos', 'tachyon.native-ui.json'), 'utf8'));
    expect(nativeBundle).toMatchObject({
        schemaVersion: 1,
        renderMode: 'native',
        entryRoute: '/',
        controller: 'tachyon.native-controller.js',
    });
    expect(nativeBundle.routes).toHaveLength(1);
    const serialized = JSON.stringify(nativeBundle.routes[0]);
    expect(serialized).toContain('"tag":"main"');
    expect(serialized).toContain('"tag":"article"');
    expect(serialized).toContain('One');
    expect(serialized).toContain('Two unavailable');
    expect(serialized).not.toMatch(/"tag":"product-card"|"tag":"loop"|"tag":"logic"/);
    const controllerSource = await readFile(path.join(root, 'dist', 'macos', nativeBundle.controller), 'utf8');
    expect(controllerSource).toContain('__tachyonNativeUI');
    expect(controllerSource).not.toMatch(/^\s*(?:import|export)\s/m);
});

test('the removed render-mode flag fails instead of selecting an app-wide WebView host', async () => {
    const root = await createFixture('<main><h1>Compatibility</h1></main>');
    const result = await runBundle(root, ['--target', 'ios', '--render-mode', 'webview', '--skip-native-host']);
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toMatch(/--render-mode has been removed/i);
});

test('an unmapped Web Component automatically becomes a local WebView boundary', async () => {
    const root = await createFixture('<main><duvay-chart title="Sales"></duvay-chart></main>');
    const result = await runBundle(root, ['--target', 'android', '--skip-native-host']);
    if (result.exitCode !== 0) throw new Error(result.stderr);
    const bundle = JSON.parse(await readFile(path.join(root, 'dist', 'android', 'tachyon.native-ui.json'), 'utf8'));
    expect(bundle).toMatchObject({ renderMode: 'native', hasWebViewFallbacks: true, webViewFallbacks: ['duvay-chart'] });
    expect(bundle.routes[0].root.children[0]).toMatchObject({ kind: 'webview', tag: 'duvay-chart' });
});

test('native mode applies semantic custom-element adapters from tac.config.js', async () => {
    const root = await createFixture('<main><w-app-bar><h1>Products</h1></w-app-bar></main>');
    await writeFile(path.join(root, 'tac.config.js'), `export default {
  nativeUIAdapters: { 'w-app-bar': 'header' },
};\n`);

    const result = await runBundle(root, ['--target', 'android', '--skip-native-host']);
    if (result.exitCode !== 0)
        throw new Error(result.stderr);
    const nativeBundle = JSON.parse(await readFile(path.join(root, 'dist', 'android', 'tachyon.native-ui.json'), 'utf8'));
    expect(nativeBundle.adapters).toEqual({ 'w-app-bar': 'header' });
    expect(nativeBundle.routes[0].root.children[0]).toMatchObject({
        kind: 'element',
        tag: 'header',
        adapter: 'w-app-bar',
    });
});

test('native-first bundling keeps native siblings and infers only the browser component boundary', async () => {
    const root = await createFixture('<main><h1>Dashboard</h1><company-chart id="sales"><canvas></canvas></company-chart><button>Refresh</button></main>');
    const result = await runBundle(root, ['--target', 'android', '--skip-native-host']);
    if (result.exitCode !== 0)
        throw new Error(result.stderr);
    const bundle = JSON.parse(await readFile(path.join(root, 'dist', 'android', 'tachyon.native-ui.json'), 'utf8'));
    expect(bundle.renderMode).toBe('native');
    expect(bundle.hasWebViewFallbacks).toBe(true);
    expect(bundle.webViewFallbacks).toEqual(['company-chart']);
    expect(bundle.routes[0].root.children).toMatchObject([
        { kind: 'element', tag: 'h1' },
        { kind: 'webview', tag: 'company-chart' },
        { kind: 'element', tag: 'button' },
    ]);
});
