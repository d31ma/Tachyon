// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from 'fs/promises';
import path from 'path';
import { tmpdir } from 'os';
import { pathToFileURL } from 'url';
import { Window } from 'happy-dom';
import { createValueEventDetail } from '../../src/runtime/dom-helpers.js';
const timedTest = /** @type {any} */ (test);
/** @type {string[]} */
const tempDirs = [];
const bundleEntrypoint = path.join(process.cwd(), 'src/cli/bundle.js');
/** @param {string} root @param {...string} segments */
function webDistPath(root, ...segments) {
    return path.join(root, 'dist', 'web', ...segments);
}
/**
 * @param {string} source
 * @returns {Record<string, unknown>}
 */
function readEmbeddedRoutes(source) {
    const match = source.match(/routeManifestJSON\s*=\s*(['"])(.*?)\1/)
        ?? source.match(/\bi\s*=\s*(['"])(.*?)\1/);
    if (match)
        return JSON.parse(match[2]);
    for (const candidate of source.matchAll(/(['"])(\{.*?\})\1/g)) {
        try {
            const parsed = JSON.parse(candidate[2]);
            if (Object.keys(parsed).some((key) => key.startsWith('/')))
                return parsed;
        }
        catch {}
    }
    throw new Error('Embedded route manifest not found');
}
async function createFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-bundle-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages', 'docs'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-bundle-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>document.title = "Fixture Home"</script><style>.hero { color: tomato; }</style><h1>Fixture Home</h1>`);
    await writeFile(path.join(root, 'client', 'pages', 'docs', 'tac.html'), `<script>document.title = "Fixture Docs"</script><p>Docs page</p>`);
    return root;
}
async function createHtmlShellFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-html-shell-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages', 'docs'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-html-shell-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>document.title = "Fixture Shell"</script>
<style>.shell { padding: 1rem; }</style>
<div class="shell">
  <p>Shell frame</p>
  <slot />
</div>`);
    await writeFile(path.join(root, 'client', 'pages', 'docs', 'tac.html'), `<script>document.title = "Fixture Docs"</script><p>Docs page</p>`);
    return root;
}
async function createSeparatedStructureFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-separated-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages', 'docs'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'hero'), { recursive: true });
    await mkdir(path.join(root, 'client', 'shared', 'scripts'), { recursive: true });
    await mkdir(path.join(root, 'client', 'shared', 'styles'), { recursive: true });
    await mkdir(path.join(root, 'server', 'routes'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-separated-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'client', 'shared', 'scripts', 'imports.js'), 'import "../styles/app.css";\n');
    await writeFile(path.join(root, 'client', 'shared', 'styles', 'app.css'), 'body { background: rgb(1, 2, 3); }\n');
    await mkdir(path.join(root, 'server', 'routes', 'GET'), { recursive: true });
    await writeFile(path.join(root, 'server', 'routes', 'GET', 'yon.js'), 'export async function handler() {\n  return { ok: true }\n}\n');
    await writeFile(path.join(root, 'client', 'components', 'hero', 'tac.html'), '<section class="hero">Hero</section>');
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>document.title = "Separated Home"</script><div class="shell"><slot /><hero /></div>`);
    await writeFile(path.join(root, 'client', 'pages', 'docs', 'tac.html'), `<script>document.title = "Separated Docs"</script><p>Docs from pages</p>`);
    return root;
}
async function createDynamicRouteFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-dynamic-prerender-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages', 'email', '_id'), { recursive: true });
    await mkdir(path.join(root, 'client', 'pages', 'folder', '_name'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-dynamic-prerender-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), '<main>Inbox shell</main>');
    await writeFile(path.join(root, 'client', 'pages', 'email', '_id', 'tac.html'), '<script>document.title = "Email"</script><article>Email route</article>');
    await writeFile(path.join(root, 'client', 'pages', 'folder', '_name', 'tac.html'), '<script>document.title = "Folder"</script><article>Folder route</article>');
    return root;
}
async function createLiteralReplacementTokenFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-literal-token-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-literal-token-fixture',
        private: true,
    }, null, 2));
    await writeFile(
        path.join(root, 'client', 'pages', 'tac.html'),
        '<main><p>Session field `$` and local field `$$` remain literal.</p></main>',
    );
    return root;
}
async function createAwaitTemplateFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-await-template-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-await-template-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `
<main>
  <p>{await message()}</p>
  <div :data-status="await status()">Status</div>
  <logic :if="await ready()">
    <strong>Ready branch</strong>
  </logic>
</main>`);
    await writeFile(path.join(root, 'client', 'pages', 'tac.js'), `
export default class extends Tac {
  async message() {
    await Promise.resolve()
    return 'Async <ready>'
  }

  async status() {
    await Promise.resolve()
    return 'online'
  }

  async ready() {
    await Promise.resolve()
    return true
  }
}
`);
    return root;
}
async function createFailingBundleWithExistingDistFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-bundle-fail-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'bad-card'), { recursive: true });
    await mkdir(webDistPath(root, 'inbox'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-bundle-fail-fixture',
        private: true,
    }, null, 2));
    await writeFile(webDistPath(root, 'inbox', 'index.html'), '<!doctype html><p>previous dist survives</p>');
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), '<bad-card />');
    await writeFile(path.join(root, 'client', 'components', 'bad-card', 'tac.html'), '<p>Invalid component path — hyphenated folder name</p>');
    return root;
}
async function createTagClassificationFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-tag-classification-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'hero', 'card'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-tag-classification-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'components', 'hero', 'card', 'tac.html'), `<article class="hero-card">Tachyon component wins</article>`);
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), [
        '<hero-card />',
        '<user-card data-kind="web-component"></user-card>',
        '<mystery>Unknown tag survives with warning</mystery>',
    ].join(''));
    return root;
}
async function createLoopEventFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-loop-event-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-loop-event-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>
let tasks = [{ text: "One", done: false }];
function toggle(index) {
    tasks = tasks.map((task, i) => i === index ? { ...task, done: !task.done } : task);
}
function status() {
    return tasks[0].done ? "done" : "pending";
}
</script>
<loop :for="let i = 0; i < tasks.length; i++">
  <button on:click="toggle(i)">{status()}</button>
</loop>`);
    return root;
}
async function createBareLoopEventFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-bare-loop-event-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-bare-loop-event-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>
let folder = 'inbox';
let folders = ['inbox', 'sent', 'trash'];
let selected = 'none';
function select(next) {
    selected = next;
}
function currentFolder() {
    return folder;
}
</script>
<loop :for="folder of folders">
  <button on:click="select(folder)">{folder}</button>
</loop>
<p>Selected {selected}</p>
<p>Prop {currentFolder()}</p>`);
    return root;
}
async function createLoopValueBindingFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-loop-value-binding-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-loop-value-binding-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>
let options = ['alpha', 'beta'];
</script>
<loop :for="option of options">
  <input :value="option" />
</loop>`);
    return root;
}
async function createValueEventFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-value-event-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-value-event-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>
let selected = 'alpha';
function choose(next) { selected = next; }
</script>
<input :value="selected" on:input="choose($event.target.value)" />
<p>Selected {selected}</p>`);
    return root;
}
async function createCheckedBindingFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-checked-binding-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-checked-binding-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>
let accepted = false;
function accept() { accepted = true; }
</script>
<input type="checkbox" :checked="accepted" on:change="accept()" />
<p>Accepted {accepted}</p>`);
    return root;
}
async function createMultiEventLoopFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-multi-event-loop-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-multi-event-loop-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>
let rows = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];
let selected = 'none';
let dragged = 'none';
function select(id) { selected = id; }
function drag(id) { dragged = id; }
</script>
<loop :for="row of rows">
  <button on:click="select(row.id)" on:dragstart="drag(row.id)">{row.id}</button>
</loop>
<p>Selected {selected}</p>
<p>Dragged {dragged}</p>`);
    return root;
}
async function createSwitchFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-switch-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-switch-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>
const Status = { Loading: 'loading', Pending: 'pending', Ready: 'ready', Error: 'error' };
let status = Status.Loading;
function setStatus(next) { status = next; }
</script>
<switch :value="status">
  <case :when="[Status.Loading, Status.Pending]">
    <button on:click="setStatus(Status.Ready)">Working</button>
  </case>
  <case :when="Status.Error">
    <p>Error</p>
  </case>
  <case default>
    <p>Ready</p>
  </case>
</switch>`);
    return root;
}
/** @param {string} template */
async function createInvalidSwitchFixture(template) {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-invalid-switch-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-invalid-switch-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), template);
    return root;
}
async function createPropRefreshFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-prop-refresh-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'data', 'loader'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-prop-refresh-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'components', 'data', 'loader', 'tac.js'), `export default class extends Tac {
    loaded = 'missing'
    constructor(props = {}) {
        super(props)
        this.loaded = String(this.props.id ?? 'missing')
    }
}
`);
    await writeFile(path.join(root, 'client', 'components', 'data', 'loader', 'tac.html'), `<p>Loaded {loaded}</p>`);
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>
let selected = 'a';
function choose(id) { selected = id; }
</script>
<button on:click="choose('b')">Choose B</button>
<data-loader :id="selected" />`);
    return root;
}
async function createKebabPropFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-kebab-prop-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'email', 'detail'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-kebab-prop-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'components', 'email', 'detail', 'tac.js'), `export default class extends Tac {
    emailId = 'missing'
    legacy = 'missing'
    constructor(props = {}) {
        super(props)
        this.emailId = String(this.props.emailId ?? 'missing')
        this.legacy = String(this.props['email-id'] ?? 'missing')
    }
}
`);
    await writeFile(path.join(root, 'client', 'components', 'email', 'detail', 'tac.html'), `<p>Email {emailId}</p><p>Legacy {legacy}</p>`);
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>let selectedMail = 'm-123'</script><email-detail :email-id="selectedMail" />`);
    return root;
}
async function createEscapingFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-escaping-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-escaping-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>
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
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-template-import-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>
const { formatLabel } = await import("./template-support");
let message = formatLabel("plugin-powered");
</script>
<p>{message}</p>`);
    await writeFile(path.join(root, 'client', 'pages', 'template-support.js'), `export function formatLabel(value) {
    return value.toUpperCase();
}
`);
    return root;
}
async function createPackageExportsFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-package-exports-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'shared', 'scripts'), { recursive: true });
    await mkdir(path.join(root, 'node_modules', 'fixture-exports', 'src'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-package-exports-fixture',
        private: true,
        dependencies: {
            'fixture-exports': '1.0.0',
        },
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>document.title = "Exports Fixture"</script><h1>Exports Fixture</h1>`);
    await writeFile(path.join(root, 'client', 'shared', 'scripts', 'imports.js'), `import { flavor } from 'fixture-exports';\nconsole.log(flavor);\n`);
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
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-main-entry-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>document.title = "Main Entry Fixture"</script><main><h1>Main Entry Fixture</h1></main>`);
    await writeFile(path.join(root, 'imports.js'), `import { bootMessage } from "./import-support.js";
import "./imports.css";

console.log(bootMessage);
document.documentElement.dataset.boot = bootMessage;
`);
    await writeFile(path.join(root, 'import-support.js'), `export const bootMessage = "booted-from-import-js";\n`);
    await writeFile(path.join(root, 'imports.css'), `body { background: rgb(1, 2, 3); }\n`);
    return root;
}
async function createCompanionScriptFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-script-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'clicker'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-companion-script-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'client', 'components', 'clicker', 'tac.html'), `<button class="clicker" on:click="increment()">{label}: {clicks}</button>`);
    await writeFile(path.join(root, 'client', 'components', 'clicker', 'tac.ts'), `export default class extends Tac {
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
    await writeFile(path.join(root, 'client', 'components', 'clicker', 'tac.css'), `.clicker {
    /* \`display: grid\` and \${theme} are author text, not JavaScript. */
    border: 2px solid tomato;
    font-weight: 700;
}`);
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<clicker label="Companion" />`);
    return root;
}
async function createDartCompanionFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-dart-companion-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'counter'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-dart-companion-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'client', 'components', 'counter', 'tac.html'), `<button on:click="increment()">Dart: {count}</button>`);
    await writeFile(path.join(root, 'client', 'components', 'counter', 'tac.dart'), `
class Counter extends Tac {
    int count = 0;

    void increment() {
        count += 1;
    }
}
`);
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<counter />`);
    return root;
}
async function createSubsetCompanionFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-subset-companion-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    const companions = {
        rust: {
            source: `struct Counter { count: i32, }
impl Counter {
    fn new() -> Self { Self { count: 0 } }
    fn increment(&mut self) { self.count += 1; }
}`,
            label: 'Rust',
            extension: 'rs',
        },
        kotlin: {
            source: `class Counter : Tac() {
    var count: Int = 0
    fun increment() { count += 1 }
}`,
            label: 'Kotlin',
            extension: 'kt',
        },
        swift: {
            source: `final class Counter: Tac {
    var count: Int = 0
    func increment() { self.count += 1 }
}`,
            label: 'Swift',
            extension: 'swift',
        },
        csharp: {
            source: `public class Counter : Tac {
    public int count = 0;
    public void increment() { this.count += 1; }
}`,
            label: 'C#',
            extension: 'cs',
        },
    };
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-subset-companion-fixture',
        private: true,
    }, null, 2));
    for (const [name, companion] of Object.entries(companions)) {
        const directory = path.join(root, 'client', 'components', name);
        await mkdir(directory, { recursive: true });
        await writeFile(path.join(directory, 'tac.html'), `<button on:click="increment()">${companion.label}: {count}</button>`);
        await writeFile(path.join(directory, `tac.${companion.extension}`), companion.source);
    }
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), '<rust /><kotlin /><swift /><csharp />');
    return root;
}
async function createGlobalTacFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-global-tac-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'badge'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-global-tac-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'components', 'badge', 'tac.html'), `<script>let label = ''</script><strong class="badge">Badge: {label}</strong>`);
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>let title = 'Global Tac'</script><badge :label="title" />`);
    return root;
}
async function createAsyncEventFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-async-event-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-async-event-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>
let status = 'idle';
async function requestMfa() {
    status = 'loading';
    await Promise.resolve();
    status = 'phone-input';
}
</script>
<button on:click="requestMfa()">Continue</button>
<p>{status}</p>`);
    return root;
}
async function createComponentSignalFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-component-signal-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'child', 'picker'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-component-signal-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'client', 'components', 'child', 'picker', 'tac.html'), `<script>
let label = 'fallback';
function choose() {
    publish('selected', { label, source: 'child-picker' });
}
</script>
<button on:click="choose()">Choose {label}</button>`);
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>
let selected = 'none';
subscribe('selected', (event) => {
    selected = event.label + ':' + event.source;
});
function receive() {
    selected = 'handled-by-signal';
}
</script>
<child-picker label="alpha" />
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
    const proc = Bun.spawn(['bun', bundleEntrypoint, '--target', 'all', '--skip-native-host'], {
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
    const home = await readFile(webDistPath(cwd, 'index.html'), 'utf8');
    const docs = await readFile(webDistPath(cwd, 'docs', 'index.html'), 'utf8');
    expect(home).toContain('<title>Fixture Home</title>');
    expect(home).toContain('>Fixture Home</h1>');
    expect(home).not.toContain('@scope');
    expect(home).toContain('<script type="module" src="/spa-renderer.js"></script>');
    expect(docs).toContain('<title>Fixture Docs</title>');
    expect(docs).toContain('>Docs page</p>');
    expect(docs).not.toContain('class="shell"');
});
timedTest('tac.bundle defaults to the web target without emitting target manifests', { timeout: 20000 }, async () => {
    const cwd = await createFixture();
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
    const html = await readFile(webDistPath(cwd, 'index.html'), 'utf8');
    const spaRenderer = await readFile(webDistPath(cwd, 'spa-renderer.js'), 'utf8');
    expect(html).toContain('<meta name="tachyon-target" content="web">');
    expect(html).toContain('<meta name="tachyon-platform" content="web">');
    expect(html).toContain('<meta name="tachyon-environment" content="web">');
    expect(html).toContain('<meta name="tachyon-os" content="web">');
    expect(spaRenderer).toContain('tachyon-target');
    expect(spaRenderer).toContain('tachyon-platform');
    expect(spaRenderer).toContain('tachyon-os');
    expect(spaRenderer).toContain('browserOS');
    await expect(Bun.file(path.join(cwd, 'dist', 'tachyon.targets.json')).exists()).resolves.toBe(false);
    await expect(Bun.file(webDistPath(cwd, 'tachyon.target.json')).exists()).resolves.toBe(false);
});
timedTest('tac.bundle accepts OS and all targets', { timeout: 60000 }, async () => {
    const cwd = await createFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint, '--targets', 'MacOS,linux,Android,iOS'], {
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
    for (const target of ['macos', 'linux', 'android', 'ios']) {
        // iOS ships the web bundle as WebBundle/ (a top-level "Resources"
        // folder breaks flat iOS bundle detection at install time).
        const assets = target === 'ios' ? 'WebBundle' : 'Resources';
        await expect(Bun.file(path.join(cwd, 'dist', target, assets, 'index.html')).exists()).resolves.toBe(true);
        await expect(Bun.file(path.join(cwd, 'dist', target, assets, 'tachyon.target.json')).exists()).resolves.toBe(false);
    }
    await expect(Bun.file(path.join(cwd, 'dist', 'tachyon.targets.json')).exists()).resolves.toBe(false);

    const allCwd = await createFixture();
    const allProc = Bun.spawn(['bun', bundleEntrypoint, '--target=all'], {
        cwd: allCwd,
        stdout: 'pipe',
        stderr: 'pipe'
    });
    const [_allStdout, allStderr, allExitCode] = await Promise.all([
        decode(allProc.stdout),
        decode(allProc.stderr),
        allProc.exited
    ]);
    if (allExitCode !== 0)
        throw new Error(allStderr);
    for (const target of ['web', 'macos', 'windows', 'linux', 'android', 'ios']) {
        const entry = target === 'web'
            ? path.join(allCwd, 'dist', target, 'index.html')
            : path.join(allCwd, 'dist', target, target === 'ios' ? 'WebBundle' : 'Resources', 'index.html');
        await expect(Bun.file(entry).exists()).resolves.toBe(true);
    }
    await expect(Bun.file(path.join(allCwd, 'dist', 'tachyon.targets.json')).exists()).resolves.toBe(false);
    await expect(Bun.file(path.join(allCwd, 'dist', 'ios', 'WebBundle', 'tachyon.target.json')).exists()).resolves.toBe(false);
});
timedTest('tac.bundle rejects unsupported targets before writing dist', { timeout: 20000 }, async () => {
    const cwd = await createFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint, '--target', 'beos'], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe'
    });
    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ]);
    expect(exitCode).not.toBe(0);
    expect(stderr).toContain("Unsupported bundle target 'beos'");
    await expect(Bun.file(path.join(cwd, 'dist', 'tachyon.targets.json')).exists()).resolves.toBe(false);
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
    const home = await readFile(webDistPath(cwd, 'index.html'), 'utf8');
    const docs = await readFile(webDistPath(cwd, 'docs', 'index.html'), 'utf8');
    await expect(Bun.file(webDistPath(cwd, 'shells.json')).exists()).resolves.toBe(false);
    expect(home).toContain('<title>Fixture Shell</title>');
    expect(home).toContain('Shell frame');
    expect(home).toContain('id="tc-page-slot"');
    expect(docs).toContain('<title>Fixture Docs</title>');
    expect(docs).toContain('Shell frame');
    expect(docs).toContain('>Docs page</p>');
});
timedTest('tac.bundle supports separated client/pages and client/components roots', { timeout: 20000 }, async () => {
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
    const home = await readFile(webDistPath(cwd, 'index.html'), 'utf8');
    const docs = await readFile(webDistPath(cwd, 'docs', 'index.html'), 'utf8');
    const mainCss = await readFile(webDistPath(cwd, 'imports.css'), 'utf8');
    expect(home).toContain('Separated Home');
    expect(home).toContain('Hero');
    expect(docs).toContain('Separated Docs');
    expect(docs).toContain('Docs from pages');
    expect(mainCss).toContain('background:#010203');
});
timedTest('tac.bundle prerenders dynamic routes into Windows-safe dist paths', { timeout: 20000 }, async () => {
    const cwd = await createDynamicRouteFixture();
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
    const routes = readEmbeddedRoutes(await readFile(webDistPath(cwd, 'spa-renderer.js'), 'utf8'));
    const email = await readFile(webDistPath(cwd, 'email', '_id', 'index.html'), 'utf8');
    const folder = await readFile(webDistPath(cwd, 'folder', '_name', 'index.html'), 'utf8');
    const emailEntries = await readdir(webDistPath(cwd, 'email'));
    const folderEntries = await readdir(webDistPath(cwd, 'folder'));
    expect(routes).toHaveProperty('/email/:id');
    expect(routes).toHaveProperty('/folder/:name');
    await expect(Bun.file(webDistPath(cwd, 'routes.json')).exists()).resolves.toBe(false);
    expect(email).toContain('Email route');
    expect(folder).toContain('Folder route');
    expect(emailEntries).toEqual(['_id']);
    expect(folderEntries).toEqual(['_name']);
});
timedTest('static prerender preserves literal dollar replacement tokens in template text', { timeout: 20000 }, async () => {
    const cwd = await createLiteralReplacementTokenFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], { cwd, stdout: 'pipe', stderr: 'pipe' });
    const [_stdout, stderr, exitCode] = await Promise.all([decode(proc.stdout), decode(proc.stderr), proc.exited]);
    if (exitCode !== 0)
        throw new Error(stderr);
    const document = await readFile(webDistPath(cwd, 'index.html'), 'utf8');
    expect(document).toContain('Session field `$` and local field `$$` remain literal.');
    expect(document.match(/<!DOCTYPE html>/g)).toHaveLength(1);
});
timedTest('Tac HTML expressions can await companion methods during render', { timeout: 20000 }, async () => {
    const cwd = await createAwaitTemplateFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], { cwd, stdout: 'pipe', stderr: 'pipe' });
    const [_stdout, stderr, exitCode] = await Promise.all([decode(proc.stdout), decode(proc.stderr), proc.exited]);
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stderr).toBe('');
    const document = await readFile(webDistPath(cwd, 'index.html'), 'utf8');
    expect(document).toContain('Async &lt;ready&gt;');
    expect(document).toContain('data-status="online"');
    expect(document).toContain('Ready branch');
});
timedTest('tac.bundle leaves the previous dist intact when a full build fails', { timeout: 20000 }, async () => {
    const cwd = await createFailingBundleWithExistingDistFixture();
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
    expect(exitCode).not.toBe(0);
    expect(stderr).toContain('Invalid Tac component path');
    const existing = await readFile(webDistPath(cwd, 'inbox', 'index.html'), 'utf8');
    expect(existing).toContain('previous dist survives');
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
    const home = await readFile(webDistPath(cwd, 'index.html'), 'utf8');
    expect(home).toContain('Tachyon component wins');
    expect(home).toContain('<user-card');
    expect(home).toContain('data-kind="web-component"');
    expect(home).toContain('<mystery');
    expect(home).toContain('Unknown tag survives with warning');
});
timedTest('tac.bundle rejects flat component template filenames', { timeout: 20000 }, async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-flat-component-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-flat-component-fixture',
        private: true,
    }, null, 2));
    // tac.html directly under components/ (no folder segment) is rejected.
    await writeFile(path.join(root, 'client', 'components', 'tac.html'), '<article>Legacy</article>');
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), '<legacy-card />');

    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd: root,
        stdout: 'pipe',
        stderr: 'pipe',
    });
    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited,
    ]);

    expect(exitCode).not.toBe(0);
    expect(stderr).toContain("Invalid Tac component path 'tac.html'");
    expect(stderr).toContain('lowercase alphanumeric folders with a tac.html template');
});
timedTest('tac.bundle rejects hyphenated component folder names', { timeout: 20000 }, async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-hyphen-component-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'panel-users'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-hyphen-component-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'client', 'components', 'panel-users', 'tac.html'), '<article>Panel users</article>');
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), '<panel-users />');

    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd: root,
        stdout: 'pipe',
        stderr: 'pipe',
    });
    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited,
    ]);

    expect(exitCode).not.toBe(0);
    expect(stderr).toContain("Invalid Tac component path 'panel-users/tac.html'");
    expect(stderr).toContain('lowercase alphanumeric folders with a tac.html template');
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
    const pageModulePath = webDistPath(cwd, 'pages', 'tac.js');
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
timedTest('bare loop variables are block scoped and do not overwrite template scope', { timeout: 20000 }, async () => {
    const cwd = await createBareLoopEventFixture();
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
    const pageModulePath = webDistPath(cwd, 'pages', 'tac.js');
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?bare-loop-event=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    const buttonIds = [...initial.matchAll(/<button[^>]* id="([^"]+)"/g)].map((match) => match[1]);
    expect(buttonIds).toHaveLength(3);
    expect(initial).toContain('Prop inbox');
    await render(buttonIds[1]);
    const updated = await render();
    expect(updated).toContain('Selected sent');
    expect(updated).toContain('Prop inbox');
});
timedTest('loop values can be used by value bindings without escaping their lexical scope', { timeout: 20000 }, async () => {
    const cwd = await createLoopValueBindingFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], { cwd, stdout: 'pipe', stderr: 'pipe' });
    const [_stdout, stderr, exitCode] = await Promise.all([decode(proc.stdout), decode(proc.stderr), proc.exited]);
    if (exitCode !== 0)
        throw new Error(stderr);
    const pageModule = await import(`${pathToFileURL(webDistPath(cwd, 'pages', 'tac.js')).href}?loop-value=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    expect(initial).toContain('value="alpha"');
    expect(initial).toContain('value="beta"');
});
timedTest('value-bound handlers retain DOM-style target access on synthetic updates', { timeout: 20000 }, async () => {
    const cwd = await createValueEventFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], { cwd, stdout: 'pipe', stderr: 'pipe' });
    const [_stdout, stderr, exitCode] = await Promise.all([decode(proc.stdout), decode(proc.stderr), proc.exited]);
    if (exitCode !== 0)
        throw new Error(stderr);
    const pageModule = await import(`${pathToFileURL(webDistPath(cwd, 'pages', 'tac.js')).href}?value-event=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    const inputId = initial.match(/<input[^>]* id="([^"]+)"/)?.[1];
    expect(inputId).toBeDefined();
    const windowInstance = new Window();
    const input = windowInstance.document.createElement('input');
    input.value = 'beta';
    await render(inputId, createValueEventDetail(/** @type {any} */ (input), /** @type {any} */ (new windowInstance.Event('input'))));
    expect(await render()).toContain('Selected beta');
    await windowInstance.happyDOM.close();
});
timedTest('checked bindings retain checkbox state across reactive rerenders', { timeout: 20000 }, async () => {
    const cwd = await createCheckedBindingFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], { cwd, stdout: 'pipe', stderr: 'pipe' });
    const [_stdout, stderr, exitCode] = await Promise.all([decode(proc.stdout), decode(proc.stderr), proc.exited]);
    if (exitCode !== 0)
        throw new Error(stderr);
    const pageModule = await import(`${pathToFileURL(webDistPath(cwd, 'pages', 'tac.js')).href}?checked=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    const inputId = initial.match(/<input[^>]* id="([^"]+)"/)?.[1];
    expect(inputId).toBeDefined();
    expect(initial).not.toMatch(/<input[^>]* checked/);
    await render(inputId, new Event('change'));
    const updated = await render();
    expect(updated).toMatch(/<input[^>]* checked/);
    expect(updated).toContain('Accepted true');
});
timedTest('multiple event handlers on one loop element keep handler counters aligned', { timeout: 20000 }, async () => {
    const cwd = await createMultiEventLoopFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], { cwd, stdout: 'pipe', stderr: 'pipe' });
    const [_stdout, stderr, exitCode] = await Promise.all([decode(proc.stdout), decode(proc.stderr), proc.exited]);
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stderr).toBe('');
    const pageModule = await import(`${pathToFileURL(webDistPath(cwd, 'pages', 'tac.js')).href}?multi-event=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    const buttonIds = [...initial.matchAll(/<button[^>]* id="([^"]+)"/g)].map((match) => match[1]);
    expect(buttonIds).toHaveLength(3);
    await render(buttonIds[1], new Event('click'));
    expect(await render()).toContain('Selected b');
});
timedTest('switch cases render grouped values and default branches', { timeout: 20000 }, async () => {
    const cwd = await createSwitchFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], { cwd, stdout: 'pipe', stderr: 'pipe' });
    const [_stdout, stderr, exitCode] = await Promise.all([decode(proc.stdout), decode(proc.stderr), proc.exited]);
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stderr).toBe('');
    const pageModule = await import(`${pathToFileURL(webDistPath(cwd, 'pages', 'tac.js')).href}?switch=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    const buttonId = initial.match(/<button[^>]* id="([^"]+)"/)?.[1];
    expect(buttonId).toBeDefined();
    expect(initial).toContain('Working');
    expect(initial).not.toContain('Ready');
    await render(buttonId, new Event('click'));
    const updated = await render();
    expect(updated).toContain('Ready');
    expect(updated).not.toContain('Working');
});
timedTest('switch validation rejects case tags outside switch', { timeout: 20000 }, async () => {
    const cwd = await createInvalidSwitchFixture('<case :when="1"><p>Invalid</p></case>');
    const proc = Bun.spawn(['bun', bundleEntrypoint], { cwd, stdout: 'pipe', stderr: 'pipe' });
    const [_stdout, stderr, exitCode] = await Promise.all([decode(proc.stdout), decode(proc.stderr), proc.exited]);
    expect(exitCode).not.toBe(0);
    expect(stderr).toContain('<case> must be inside <switch>');
});
timedTest('switch validation rejects duplicate literal cases', { timeout: 20000 }, async () => {
    const cwd = await createInvalidSwitchFixture('<switch :value="1"><case :when="1">One</case><case :when="1">Again</case></switch>');
    const proc = Bun.spawn(['bun', bundleEntrypoint], { cwd, stdout: 'pipe', stderr: 'pipe' });
    const [_stdout, stderr, exitCode] = await Promise.all([decode(proc.stdout), decode(proc.stderr), proc.exited]);
    expect(exitCode).not.toBe(0);
    expect(stderr).toContain('duplicate literal case value');
});
timedTest('component factories refresh when parent props change', { timeout: 20000 }, async () => {
    const cwd = await createPropRefreshFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], { cwd, stdout: 'pipe', stderr: 'pipe' });
    const [_stdout, stderr, exitCode] = await Promise.all([decode(proc.stdout), decode(proc.stderr), proc.exited]);
    if (exitCode !== 0)
        throw new Error(stderr);
    expect(stderr).toBe('');
    const pageModule = await import(`${pathToFileURL(webDistPath(cwd, 'pages', 'tac.js')).href}?prop-refresh=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    const buttonId = initial.match(/<button[^>]* id="([^"]+)"/)?.[1];
    expect(buttonId).toBeTruthy();
    expect(initial).toContain('Loaded a');
    await render(buttonId);
    expect(await render()).toContain('Loaded b');
});
timedTest('component props expose kebab-case attributes as camelCase aliases', { timeout: 20000 }, async () => {
    const cwd = await createKebabPropFixture();
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
    const pageModulePath = webDistPath(cwd, 'pages', 'tac.js');
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?kebab-prop=${Date.now()}`);
    const render = await pageModule.default();
    const html = await render();
    expect(html).toContain('Email m-123');
    expect(html).toContain('Legacy m-123');
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
    const pageModulePath = webDistPath(cwd, 'pages', 'tac.js');
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
    const pageModulePath = webDistPath(cwd, 'pages', 'tac.js');
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
    const modulePath = webDistPath(cwd, 'shared', 'modules', 'fixture-exports.js');
    const bundledModule = await readFile(modulePath, 'utf8');
    expect(bundledModule).toContain('exports-aware');
    const loaded = await import(`${pathToFileURL(modulePath).href}?exports=${Date.now()}`);
    expect(loaded.flavor).toBe('exports-aware');
    expect(loaded.default()).toBe('exports-aware');
});
timedTest('tac.bundle bundles typed import entrypoints and emits imports.css when imported', { timeout: 20000 }, async () => {
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
    const html = await readFile(webDistPath(cwd, 'index.html'), 'utf8');
    const bundledMain = await readFile(webDistPath(cwd, 'imports.js'), 'utf8');
    const bundledCss = await readFile(webDistPath(cwd, 'imports.css'), 'utf8');
    expect(html).toContain('<link rel="stylesheet" href="/imports.css">');
    expect(html).toContain('<script type="module" src="/spa-renderer.js"></script>');
    expect(html).toContain('<script type="module" src="/imports.js"></script>');
    expect(html.indexOf('/spa-renderer.js')).toBeLessThan(html.indexOf('/imports.js'));
    expect(bundledMain).toContain('booted-from-import-js');
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
    const home = await readFile(webDistPath(cwd, 'index.html'), 'utf8');
    const pageModule = await readFile(webDistPath(cwd, 'pages', 'tac.js'), 'utf8');
    const componentModule = await readFile(webDistPath(cwd, 'components', 'badge', 'tac.js'), 'utf8');
    expect(home).toContain('Badge: Global Tac');
    expect(pageModule).toContain('register("/pages/tac.js"');
    expect(componentModule).toContain('register("/components/badge/tac.js"');
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
    const pageModulePath = webDistPath(cwd, 'pages', 'tac.js');
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
    const pageModulePath = webDistPath(cwd, 'pages', 'tac.js');
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?companion-script=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    const buttonId = initial.match(/<button[^>]* id="([^"]+)"/)?.[1];
    expect(initial).toContain('Companion: 0');
    expect(initial).toContain('data-tac-scope="clicker"');
    expect(initial).toContain('@scope ([data-tac-scope="clicker"])');
    expect(initial).toContain('`display: grid` and ${theme} are author text, not JavaScript.');
    expect(initial).toContain('border: 2px solid tomato;');
    expect(initial).toContain('font-weight: 700;');
    expect(buttonId).toBeDefined();
    const updated = await render(buttonId);
    expect(updated).toContain('Clicked: 1');
});
timedTest('Dart component companions compile into normal reactive Tac modules', { timeout: 30000 }, async () => {
    if (!Bun.which('dart'))
        return;
    const cwd = await createDartCompanionFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe',
    });
    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited,
    ]);
    if (exitCode !== 0)
        throw new Error(stderr);
    const pageModulePath = webDistPath(cwd, 'pages', 'tac.js');
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?dart-companion=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    const buttonId = initial.match(/<button[^>]* id="([^"]+)"/)?.[1];
    expect(initial).toContain('Dart: 0');
    expect(buttonId).toBeDefined();
    const updated = await render(buttonId);
    expect(updated).toContain('Dart: 1');
});
timedTest('one Dart companion is emitted for every native and web target', { timeout: 60000 }, async () => {
    if (!Bun.which('dart'))
        return;
    const cwd = await createDartCompanionFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint, '--target', 'all', '--skip-native-host'], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe',
    });
    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited,
    ]);
    if (exitCode !== 0)
        throw new Error(stderr);
    for (const target of ['web', 'macos', 'windows', 'linux', 'ios', 'android']) {
        const adapter = await readFile(path.join(cwd, 'dist', target, 'components', 'counter', 'tac.js'), 'utf8');
        const runtime = await readFile(path.join(cwd, 'dist', target, 'components', 'counter', 'tac.dart.js'), 'utf8');
        expect(adapter).toContain('tac.dart.js');
        expect(runtime).toContain('dartProgram');
    }
});
timedTest('in-house Rust, Kotlin, Swift, and C# companions bundle as portable Tac controllers', { timeout: 30000 }, async () => {
    const cwd = await createSubsetCompanionFixture();
    const proc = Bun.spawn(['bun', bundleEntrypoint, '--target', 'all', '--skip-native-host'], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe',
    });
    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited,
    ]);
    if (exitCode !== 0)
        throw new Error(stderr);
    const pageModulePath = webDistPath(cwd, 'pages', 'tac.js');
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?subset-companions=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    for (const label of ['Rust', 'Kotlin', 'Swift', 'C#'])
        expect(initial).toContain(`${label}: 0`);
    for (const target of ['web', 'macos', 'windows', 'linux', 'ios', 'android']) {
        for (const language of ['rust', 'kotlin', 'swift', 'csharp']) {
            const output = await readFile(path.join(cwd, 'dist', target, 'components', language, 'tac.js'), 'utf8');
            expect(output).toContain('__tc_signal_publish_fields__');
        }
    }
});
timedTest('components can publish signals handled by their parent page', { timeout: 20000 }, async () => {
    const cwd = await createComponentSignalFixture();
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
        const pageModulePath = webDistPath(cwd, 'pages', 'tac.js');
        const pageModule = await import(`${pathToFileURL(pageModulePath).href}?component-signal=${Date.now()}`);
        const render = await pageModule.default();
        const initial = await render();
        document.body.innerHTML = initial;
        const button = document.querySelector('button');
        expect(button?.id).toBeTruthy();
        expect(initial).toContain('Selected none');
        if (!button)
            throw new Error('Expected button to exist in the signal component fixture');
        await render(button.id, new windowInstance.MouseEvent('click'));
        expect(await render()).toContain('Selected alpha:child-picker');
    }
    finally {
        await windowInstance.happyDOM.close();
        Object.assign(globalThis, previousGlobals);
    }
});

timedTest('spa prehydration skips malformed persisted fields without blocking valid ones', { timeout: 20000 }, async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-spa-prehydrate-'));
    tempDirs.push(root);
    const runtimeSource = await readFile(path.join(process.cwd(), 'src/runtime/spa-renderer.js'), 'utf8');
    const prehydrateMarker = '\nprehydratePersistentText();';
    const prehydrateIndex = runtimeSource.indexOf(prehydrateMarker);
    if (prehydrateIndex < 0)
        throw new Error('Expected spa-renderer.js to call prehydratePersistentText()');
    const prehydrateEnd = prehydrateIndex + prehydrateMarker.length;
    const prehydrateModulePath = path.join(root, 'prehydrate.js');
    await writeFile(prehydrateModulePath, runtimeSource
        .slice(0, prehydrateEnd)
        .replace(/^import .*?;\n/gm, ''));
    const windowInstance = new Window();
    /** @type {Record<string, unknown>} */
    const previousGlobals = {
        window: globalThis.window,
        document: globalThis.document,
        HTMLElement: globalThis.HTMLElement,
        sessionStorage: globalThis.sessionStorage,
        localStorage: globalThis.localStorage,
        fetch: globalThis.fetch,
        history: globalThis.history,
        location: globalThis.location,
        addEventListener: globalThis.addEventListener,
        CustomEvent: globalThis.CustomEvent,
        SyntaxError: globalThis.SyntaxError,
    };
    try {
        windowInstance.SyntaxError = SyntaxError;
        Object.assign(globalThis, {
            window: windowInstance,
            document: windowInstance.document,
            HTMLElement: windowInstance.HTMLElement,
            sessionStorage: windowInstance.sessionStorage,
            localStorage: windowInstance.localStorage,
            history: windowInstance.history,
            location: windowInstance.location,
            addEventListener: windowInstance.addEventListener.bind(windowInstance),
            CustomEvent: windowInstance.CustomEvent,
            SyntaxError,
            fetch: /** @type {typeof fetch} */ (async (input) => {
                return new Response('', { status: 404 });
            }),
        });
        document.body.innerHTML = `
            <div id="fixture" data-tac-module="/pages/tac.js">
                <span id="bad" data-tac-persist-field="$$theme">light</span>
                <span id="good" data-tac-persist-field="$clicks">0</span>
            </div>
        `;
        windowInstance.localStorage.setItem('tac:/pages/tac.js:fixture:$$theme', 'not-json{{{');
        windowInstance.sessionStorage.setItem('tac:/pages/tac.js:fixture:$clicks', JSON.stringify(7));

        await import(`${pathToFileURL(prehydrateModulePath).href}?prehydrate=${Date.now()}`);

        expect(document.getElementById('bad')?.textContent).toBe('light');
        expect(document.getElementById('good')?.textContent).toBe('7');
    }
    finally {
        await windowInstance.happyDOM.close();
        Object.assign(globalThis, previousGlobals);
        for (const key of Object.keys(previousGlobals)) {
            if (previousGlobals[key] === undefined)
                Reflect.deleteProperty(globalThis, key);
        }
    }
});

// ── regression: issue #57 — large companion scripts must include class Tac ────
async function createLargeCompanionFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-large-companion-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'big'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-large-companion-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'client', 'components', 'big', 'tac.html'), `<strong class="big-widget">{label}</strong>`);
    // Companion script exceeding ~1.3 KB to trigger the bundler size threshold
    const padding = '// ' + 'x'.repeat(1500);
    await writeFile(path.join(root, 'client', 'components', 'big', 'tac.js'), `${padding}
export default class extends Tac {
    label = 'BigWidget'

    constructor(props = {}) {
        super(props)
    }
}
`);
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<big />`);
    return root;
}

timedTest('large companion scripts over 1.3KB include class Tac definition', { timeout: 20000 }, async () => {
    const cwd = await createLargeCompanionFixture();
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
    const distComponentPath = webDistPath(cwd, 'components', 'big', 'tac.js');
    const componentSource = await readFile(distComponentPath, 'utf8');
    expect(componentSource).toContain('class Tac{');
    expect(componentSource).toContain('extends Tac');
});

// ── regression: issue #58 — component import bindings accept props and call factory ────
async function createComponentImportBindingFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-import-binding-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'greeting'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-import-binding-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'client', 'components', 'greeting', 'tac.html'), `<span class="greeting">Hello {name}</span>`);
    await writeFile(path.join(root, 'client', 'components', 'greeting', 'tac.js'), `export default class extends Tac {
    name = 'World'
    constructor(props = {}) {
        super(props)
        this.name = String(this.props.name ?? 'World')
    }
}
`);
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<greeting name="Tachyon" />`);
    return root;
}

timedTest('page-level component import bindings accept props and call the factory', { timeout: 20000 }, async () => {
    const cwd = await createComponentImportBindingFixture();
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
    // Verify the generated page module has the fixed binding pattern: (p) => import(...).then(async (m) => { const f = m.default || m; return await f(p) })
    const pageModulePath = webDistPath(cwd, 'pages', 'tac.js');
    const pageSource = await readFile(pageModulePath, 'utf8');
    expect(pageSource).toMatch(/\(\w\)=>import\("[^"]+"\)\.then\(async\(\w\)=>\{/);
    // Functional test: import and render to verify the factory is called correctly
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?import-binding=${Date.now()}`);
    const render = await pageModule.default();
    const initial = await render();
    expect(initial).toContain('Hello Tachyon');
});
