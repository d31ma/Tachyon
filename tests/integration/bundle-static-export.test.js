// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'fs/promises';
import path from 'path';
import { tmpdir } from 'os';
import { pathToFileURL } from 'url';
import { Window } from 'happy-dom';
const timedTest = /** @type {any} */ (test);
/** @type {string[]} */
const tempDirs = [];
const bundleEntrypoint = path.join(process.cwd(), 'src/cli/bundle.js');
async function createFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-bundle-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes', 'docs'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-bundle-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'routes', 'index.html'), `<script>document.title = "Fixture Home"</script><style>.hero { color: tomato; }</style><h1>Fixture Home</h1>`);
    await writeFile(path.join(root, 'routes', 'docs', 'index.html'), `<script>document.title = "Fixture Docs"</script><p>Docs page</p>`);
    return root;
}
async function createHtmlShellFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-html-shell-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes', 'docs'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-html-shell-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'routes', 'index.html'), `<script>document.title = "Fixture Shell"</script>
<style>.shell { padding: 1rem; }</style>
<div class="shell">
  <p>Shell frame</p>
  <slot />
</div>`);
    await writeFile(path.join(root, 'routes', 'docs', 'index.html'), `<script>document.title = "Fixture Docs"</script><p>Docs page</p>`);
    return root;
}
async function createSeparatedStructureFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-separated-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'browser', 'pages', 'docs'), { recursive: true });
    await mkdir(path.join(root, 'browser', 'components'), { recursive: true });
    await mkdir(path.join(root, 'browser', 'shared', 'scripts'), { recursive: true });
    await mkdir(path.join(root, 'browser', 'shared', 'styles'), { recursive: true });
    await mkdir(path.join(root, 'server', 'routes'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-separated-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'browser', 'shared', 'scripts', 'main.js'), 'import "../styles/app.css";\n');
    await writeFile(path.join(root, 'browser', 'shared', 'styles', 'app.css'), 'body { background: rgb(1, 2, 3); }\n');
    await writeFile(path.join(root, 'server', 'routes', 'GET'), '#!/usr/bin/env bun\nBun.stdout.write(JSON.stringify({ ok: true }))\n');
    await writeFile(path.join(root, 'browser', 'components', 'hero.html'), '<section class="hero">Hero</section>');
    await writeFile(path.join(root, 'browser', 'pages', 'index.html'), `<script>document.title = "Separated Home"</script><div class="shell"><slot /><hero /></div>`);
    await writeFile(path.join(root, 'browser', 'pages', 'docs', 'index.html'), `<script>document.title = "Separated Docs"</script><p>Docs from pages</p>`);
    return root;
}
async function createTagClassificationFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-tag-classification-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes'), { recursive: true });
    await mkdir(path.join(root, 'components'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-tag-classification-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'components', 'hero-card.html'), `<article class="hero-card">Tachyon component wins</article>`);
    await writeFile(path.join(root, 'routes', 'index.html'), [
        '<hero-card />',
        '<user-card data-kind="web-component"></user-card>',
        '<mystery>Unknown tag survives with warning</mystery>',
    ].join(''));
    return root;
}
async function createLoopEventFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-loop-event-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-loop-event-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'routes', 'index.html'), `<script>
let tasks = [{ text: "One", done: false }];
function toggle(index) {
    tasks = tasks.map((task, i) => i === index ? { ...task, done: !task.done } : task);
}
function status() {
    return tasks[0].done ? "done" : "pending";
}
</script>
<loop :for="let i = 0; i < tasks.length; i++">
  <button @click="toggle(i)">{status()}</button>
</loop>`);
    return root;
}
async function createEscapingFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-escaping-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-escaping-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'routes', 'index.html'), `<script>
let message = '<img src=x onerror=alert(1)>';
let title = '" onfocus="alert(1)';
let trusted = '<strong>Trusted raw HTML</strong>';
</script>
<p>{message}</p>
<div :title="title">Hover me</div>
<section>{!trusted}</section>`);
    return root;
}
async function createTemplateImportFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-template-import-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-template-import-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'routes', 'index.html'), `<script>
const { formatLabel } = await import("./template-support");
let message = formatLabel("plugin-powered");
</script>
<p>{message}</p>`);
    await writeFile(path.join(root, 'routes', 'template-support.js'), `export function formatLabel(value) {
    return value.toUpperCase();
}
`);
    return root;
}
async function createPackageExportsFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-package-exports-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes'), { recursive: true });
    await mkdir(path.join(root, 'node_modules', 'fixture-exports', 'src'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-package-exports-fixture',
        private: true,
        dependencies: {
            'fixture-exports': '1.0.0',
        },
    }, null, 2));
    await writeFile(path.join(root, 'routes', 'index.html'), `<script>document.title = "Exports Fixture"</script><h1>Exports Fixture</h1>`);
    await writeFile(path.join(root, 'node_modules', 'fixture-exports', 'package.json'), JSON.stringify({
        name: 'fixture-exports',
        version: '1.0.0',
        type: 'module',
        exports: './src/index.js',
    }, null, 2));
    await writeFile(path.join(root, 'node_modules', 'fixture-exports', 'src', 'index.js'), 'export const flavor = "exports-aware"; export default function label() { return flavor; }\n');
    return root;
}
async function createMainEntrypointFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-main-entry-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-main-entry-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'routes', 'index.html'), `<script>document.title = "Main Entry Fixture"</script><main><h1>Main Entry Fixture</h1></main>`);
    await writeFile(path.join(root, 'main.js'), `import { bootMessage } from "./main-support.js";
import "./main.css";

console.log(bootMessage);
document.documentElement.dataset.boot = bootMessage;
`);
    await writeFile(path.join(root, 'main-support.js'), `export const bootMessage = "booted-from-main-js";\n`);
    await writeFile(path.join(root, 'main.css'), `body { background: rgb(1, 2, 3); }\n`);
    return root;
}
async function createCompanionScriptFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-script-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes'), { recursive: true });
    await mkdir(path.join(root, 'components'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-companion-script-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'components', 'clicker.html'), `<button class="clicker" @click="increment()">{label}: {clicks}</button>`);
    await writeFile(path.join(root, 'components', 'clicker.ts'), `export default class extends Tac {
    clicks: number = 0
    label = 'Ready'

    constructor(props: Record<string, unknown> = {}) {
        super(props)
        this.label = String(this.props.label ?? 'Ready')
    }

    increment() {
        this.clicks += 1
        this.label = 'Clicked'
    }
}
`);
    await writeFile(path.join(root, 'components', 'clicker.css'), `.clicker { border: 2px solid tomato; font-weight: 700; }`);
    await writeFile(path.join(root, 'routes', 'index.html'), `<clicker label="Companion" />`);
    return root;
}
async function createGlobalTacFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-global-tac-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes'), { recursive: true });
    await mkdir(path.join(root, 'components'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-global-tac-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'components', 'badge.html'), `<script>let label = ''</script><strong class="badge">Badge: {label}</strong>`);
    await writeFile(path.join(root, 'routes', 'index.html'), `<script>let title = 'Global Tac'</script><badge :label="title" />`);
    return root;
}
async function createAsyncEventFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-async-event-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-async-event-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'routes', 'index.html'), `<script>
let status = 'idle';
async function requestMfa() {
    status = 'loading';
    await Promise.resolve();
    status = 'phone-input';
}
</script>
<button @click="requestMfa()">Continue</button>
<p>{status}</p>`);
    return root;
}
async function createComponentEmitFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-component-emit-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes'), { recursive: true });
    await mkdir(path.join(root, 'components'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-component-emit-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'components', 'child-picker.html'), `<script>
let label = 'fallback';
function choose() {
    emit('selected', { label, source: 'child-picker' });
}
</script>
<button @click="choose()">Choose {label}</button>`);
    await writeFile(path.join(root, 'routes', 'index.html'), `<script>
let selected = 'none';
function receive(event) {
    selected = event.detail.label + ':' + event.detail.source;
}
</script>
<child-picker label="alpha" @selected="receive($event)" />
<p>Selected {selected}</p>`);
    return root;
}
/** @param {ReadableStream<Uint8Array> | null | undefined} stream */
async function decode(stream) {
    if (!stream)
        return '';
    return await new Response(stream).text();
}
afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});
timedTest('tac.bundle prerenders HTML routes into static documents', { timeout: 20000 }, async () => {
    const cwd = await createFixture();
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
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stderr).toBe('');
    expect(stdout).toContain('Bundle completed');
    const home = await readFile(path.join(cwd, 'dist', 'index.html'), 'utf8');
    const docs = await readFile(path.join(cwd, 'dist', 'docs', 'index.html'), 'utf8');
    expect(home).toContain('<title>Fixture Home</title>');
    expect(home).toContain('>Fixture Home</h1>');
    expect(home).not.toContain('@scope');
    expect(home).toContain('<script type="module" src="/spa-renderer.js"></script>');
    expect(docs).toContain('<title>Fixture Docs</title>');
    expect(docs).toContain('>Docs page</p>');
    expect(docs).not.toContain('class="shell"');
});
timedTest('ancestor HTML files with <slot /> act as shells for descendant HTML routes', { timeout: 20000 }, async () => {
    const cwd = await createHtmlShellFixture();
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
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stdout).toContain('Bundle completed');
    expect(stderr).toBe('');
    const layouts = JSON.parse(await readFile(path.join(cwd, 'dist', 'shells.json'), 'utf8'));
    const home = await readFile(path.join(cwd, 'dist', 'index.html'), 'utf8');
    const docs = await readFile(path.join(cwd, 'dist', 'docs', 'index.html'), 'utf8');
    expect(layouts['/']).toEqual({ path: '/pages/index.js', allowSelf: false });
    expect(home).toContain('<title>Fixture Shell</title>');
    expect(home).toContain('Shell frame');
    expect(home).toContain('id="ty-layout-slot"');
    expect(docs).toContain('<title>Fixture Docs</title>');
    expect(docs).toContain('Shell frame');
    expect(docs).toContain('>Docs page</p>');
});
timedTest('tac.bundle supports separated browser/pages and browser/components roots', { timeout: 20000 }, async () => {
    const cwd = await createSeparatedStructureFixture();
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
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stdout).toContain('Bundle completed');
    expect(stderr).toBe('');
    const home = await readFile(path.join(cwd, 'dist', 'index.html'), 'utf8');
    const docs = await readFile(path.join(cwd, 'dist', 'docs', 'index.html'), 'utf8');
    const mainCss = await readFile(path.join(cwd, 'dist', 'main.css'), 'utf8');
    expect(home).toContain('Separated Home');
    expect(home).toContain('Hero');
    expect(docs).toContain('Separated Docs');
    expect(docs).toContain('Docs from pages');
    expect(mainCss).toContain('background:#010203');
});
timedTest('tac.bundle classifies component, web component, native, and unknown tags by priority', { timeout: 20000 }, async () => {
    const cwd = await createTagClassificationFixture();
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
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stdout).toContain('Bundle completed');
    expect(stderr).toContain('Unknown element tag');
    expect(stderr).toContain('mystery');
    const home = await readFile(path.join(cwd, 'dist', 'index.html'), 'utf8');
    expect(home).toContain('Tachyon component wins');
    expect(home).toContain('<user-card');
    expect(home).toContain('data-kind="web-component"');
    expect(home).toContain('<mystery');
    expect(home).toContain('Unknown tag survives with warning');
});
timedTest('loop-scoped event handlers can access loop variables when rerendered', { timeout: 20000 }, async () => {
    const cwd = await createLoopEventFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe'
    });
    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ]);
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stderr).toBe('');
    const pageModulePath = path.join(cwd, 'dist', 'pages', 'index.js');
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?loop-event=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    const buttonId = initial.match(/<button[^>]* id="([^"]+)"/)?.[1];
    expect(buttonId).toBeDefined();
    expect(initial).toContain('pending');
    await render(buttonId);
    const updated = await render();
    expect(updated).toContain('done');
});
timedTest('template interpolation and dynamic attributes are escaped by default', { timeout: 20000 }, async () => {
    const cwd = await createEscapingFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe'
    });
    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ]);
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stderr).toBe('');
    const pageModulePath = path.join(cwd, 'dist', 'pages', 'index.js');
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?escaping=${Date.now()}`);
    const render = await pageModule.default();
    const html = await render();
    expect(html).toContain('&lt;img src=x onerror=alert(1)&gt;');
    expect(html).not.toContain('<img src=x onerror=alert(1)>');
    expect(html).toContain('title="&quot; onfocus=&quot;alert(1)"');
    expect(html).not.toContain('title="" onfocus="alert(1)"');
    expect(html).toContain('<strong>Trusted raw HTML</strong>');
});
timedTest('Tac template scripts can bundle relative imports from their source directory', { timeout: 20000 }, async () => {
    const cwd = await createTemplateImportFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe'
    });
    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ]);
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stderr).toBe('');
    const pageModulePath = path.join(cwd, 'dist', 'pages', 'index.js');
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?template-import=${Date.now()}`);
    const render = await pageModule.default();
    expect(await render()).toContain('PLUGIN-POWERED');
});
timedTest('tac.bundle resolves dependency entrypoints via package exports', { timeout: 20000 }, async () => {
    const cwd = await createPackageExportsFixture();
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
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stdout).toContain('Bundle completed');
    expect(stderr).toBe('');
    const modulePath = path.join(cwd, 'dist', 'modules', 'fixture-exports.js');
    const bundledModule = await readFile(modulePath, 'utf8');
    expect(bundledModule).toContain('exports-aware');
    const loaded = await import(`${pathToFileURL(modulePath).href}?exports=${Date.now()}`);
    expect(loaded.flavor).toBe('exports-aware');
    expect(loaded.default()).toBe('exports-aware');
});
timedTest('tac.bundle bundles typed main entrypoints and emits main.css when imported', { timeout: 20000 }, async () => {
    const cwd = await createMainEntrypointFixture();
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
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stdout).toContain('Bundle completed');
    expect(stderr).toBe('');
    const html = await readFile(path.join(cwd, 'dist', 'index.html'), 'utf8');
    const bundledMain = await readFile(path.join(cwd, 'dist', 'main.js'), 'utf8');
    const bundledCss = await readFile(path.join(cwd, 'dist', 'main.css'), 'utf8');
    expect(html).toContain('<link rel="stylesheet" href="/main.css">');
    expect(html).toContain('<script type="module" src="/spa-renderer.js"></script>');
    expect(html).toContain('<script type="module" src="/main.js"></script>');
    expect(html.indexOf('/spa-renderer.js')).toBeLessThan(html.indexOf('/main.js'));
    expect(bundledMain).toContain('booted-from-main-js');
    expect(bundledCss).toContain('background:#010203');
});
timedTest('TAC_FORMAT=global emits registry modules that prerender successfully', { timeout: 20000 }, async () => {
    const cwd = await createGlobalTacFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        env: {
            ...process.env,
            TAC_FORMAT: 'global',
        },
        stdout: 'pipe',
        stderr: 'pipe'
    });
    const [stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ]);
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stdout).toContain('Bundle completed');
    expect(stderr).toBe('');
    const home = await readFile(path.join(cwd, 'dist', 'index.html'), 'utf8');
    const pageModule = await readFile(path.join(cwd, 'dist', 'pages', 'index.js'), 'utf8');
    const componentModule = await readFile(path.join(cwd, 'dist', 'components', 'badge.js'), 'utf8');
    expect(home).toContain('Badge: Global Tac');
    expect(pageModule).toContain('register("/pages/index.js"');
    expect(componentModule).toContain('register("/components/badge.js"');
    expect(pageModule).not.toContain('export default');
    expect(componentModule).not.toContain('export default');
});
timedTest('async event handlers are awaited before Tac rerenders', { timeout: 20000 }, async () => {
    const cwd = await createAsyncEventFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe'
    });
    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ]);
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stderr).toBe('');
    const pageModulePath = path.join(cwd, 'dist', 'pages', 'index.js');
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?async-event=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    const buttonId = initial.match(/<button[^>]* id="([^"]+)"/)?.[1];
    expect(buttonId).toBeDefined();
    expect(initial).toContain('>idle</p>');
    const updated = await render(buttonId);
    expect(updated).toContain('>phone-input</p>');
    expect(updated).not.toContain('>loading</p>');
});
timedTest('component companion scripts in JavaScript or TypeScript and scoped css are bundled with Tac templates', { timeout: 20000 }, async () => {
    const cwd = await createCompanionScriptFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe'
    });
    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ]);
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stderr).toBe('');
    const pageModulePath = path.join(cwd, 'dist', 'pages', 'index.js');
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?companion-script=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    const buttonId = initial.match(/<button[^>]* id="([^"]+)"/)?.[1];
    expect(initial).toContain('Companion: 0');
    expect(initial).toContain('data-tac-scope="clicker"');
    expect(initial).toContain('@scope ([data-tac-scope="clicker"])');
    expect(initial).toContain('.clicker { border: 2px solid tomato; font-weight: 700; }');
    expect(buttonId).toBeDefined();
    const updated = await render(buttonId);
    expect(updated).toContain('Clicked: 1');
});
timedTest('components can emit custom events handled by their parent wrapper', { timeout: 20000 }, async () => {
    const cwd = await createComponentEmitFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe'
    });
    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ]);
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stderr).toBe('');
    const pageModulePath = path.join(cwd, 'dist', 'pages', 'index.js');
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?component-emit=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    const windowInstance = new Window();
    const previousGlobals = {
        window: globalThis.window,
        document: globalThis.document,
        CustomEvent: globalThis.CustomEvent,
    };
    try {
        Object.assign(windowInstance, {
            SyntaxError,
        });
        Object.assign(globalThis, {
            window: windowInstance,
            document: windowInstance.document,
            CustomEvent: windowInstance.CustomEvent,
        });
        document.body.innerHTML = initial;
        const wrapperId = initial.match(/<div id="([^"]+)"[^>]*@selected/)?.[1];
        const wrapper = wrapperId ? document.getElementById(wrapperId) : null;
        const button = document.querySelector('button');
        expect(wrapper?.id).toBeTruthy();
        expect(button?.id).toBeTruthy();
        expect(initial).toContain('Selected none');
        if (!wrapper || !button)
            throw new Error('Expected wrapper and button to exist in the emitted component fixture');
        const received = new Promise((resolve) => {
            wrapper.addEventListener('selected', async (event) => {
                const selectedEvent = /** @type {CustomEvent<{ label: string, source: string }>} */ (event);
                await render(wrapper.id, selectedEvent);
                resolve(selectedEvent.detail);
            });
        });
        await render(button.id, new windowInstance.MouseEvent('click'));
        expect(await received).toEqual({ label: 'alpha', source: 'child-picker' });
        expect(await render()).toContain('Selected alpha:child-picker');
    }
    finally {
        await windowInstance.happyDOM.close();
        Object.assign(globalThis, previousGlobals);
    }
});
