// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { mkdtemp, mkdir, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import { pathToFileURL } from 'url';
import { Window } from 'happy-dom';
import { findEventTarget, morphChildren, parseFragment } from '../../src/runtime/dom-helpers.js';

const bundleEntrypoint = path.join(process.cwd(), 'src/cli/bundle.js');
/** @type {string[]} */
const tempDirs = [];

afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((directory) => rm(directory, { recursive: true, force: true })));
    Reflect.deleteProperty(globalThis, '__issue116Hits');
});

/** @param {ReadableStream<Uint8Array> | null | undefined} stream */
function decode(stream) {
    return stream ? new Response(stream).text() : Promise.resolve('');
}

/** @param {string} root @param {...string} segments */
function webDistPath(root, ...segments) {
    return path.join(root, 'dist', 'web', ...segments);
}

async function bundleFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-issue-116-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await mkdir(path.join(root, 'client', 'components', 'row', 'card'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-issue-116-fixture',
        private: true,
    }));
    await writeFile(path.join(root, 'client', 'components', 'row', 'card', 'tac.html'), `
<article class="component-row" :data-row="item.id" tabindex="0"
  on:click="globalThis.__issue116Hits.push('select:' + item.id)"
  on:keydown="event.key === 'Enter' && globalThis.__issue116Hits.push('key:' + item.id)">
  <span>{item.id}</span>
  <button class="component-archive" :data-row="item.id"
    on:click="globalThis.__issue116Hits.push('archive:' + item.id)">Archive</button>
</article>`);
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), `<script>
let rows = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];
let actions = [];
function replaceRows() { rows = rows.map((row) => ({ ...row })); }
function filterFirst() { rows = rows.filter((row) => row.id !== 'a'); }
function select(id) { actions.push('select:' + id); }
function archive(id) { actions.push('archive:' + id); }
function activate(id, event) { if (event.key === 'Enter') actions.push('key:' + id); }
</script>
<button class="replace" on:click="replaceRows()">Replace</button>
<button class="filter" on:click="filterFirst()">Filter</button>
<loop :for="row of rows">
  <article class="row" :data-row="row.id" tabindex="0" on:click="select(row.id)" on:keydown="activate(row.id, $event)">
    <span>{row.id}</span>
    <button class="archive" :data-row="row.id" on:click="archive(row.id)">Archive</button>
  </article>
  <row-card :item="row" />
</loop>
<output>{actions.join('|')}</output>`);
    const process = Bun.spawn(['bun', bundleEntrypoint], { cwd: root, stdout: 'pipe', stderr: 'pipe' });
    const [_stdout, stderr, exitCode] = await Promise.all([decode(process.stdout), decode(process.stderr), process.exited]);
    if (exitCode !== 0)
        throw new Error(stderr);
    return root;
}

/** @param {Element} element */
function dispatchId(element) {
    return element.getAttribute('data-tac-id') || element.id;
}

/**
 * Exercises the public compiler output and the shipped DOM/event lookup rather
 * than retaining references to nodes from an earlier render.
 * @param {Document} document
 * @param {(id?: string | null, event?: unknown) => Promise<string>} render
 * @param {Element} origin
 * @param {string} eventName
 * @param {Event} event
 */
async function dispatchAndPatch(document, render, origin, eventName, event) {
    const target = findEventTarget(origin, eventName);
    if (!target)
        throw new Error(`Current ${eventName} target has no delegated handler marker: ${origin?.outerHTML ?? '<missing>'}`);
    await render(dispatchId(/** @type {Element} */ (target)), event);
    morphChildren(document.body, parseFragment(await render()));
}

test('loop root, nested, and keyboard handlers survive repeated replacement and filtering (#116)', async () => {
    const root = await bundleFixture();
    const moduleURL = `${pathToFileURL(webDistPath(root, 'pages', 'tac.js')).href}?issue-116=${Date.now()}`;
    const pageModule = await import(moduleURL);
    const render = await pageModule.default();
    const windowInstance = new Window({ url: 'http://localhost/' });
    Object.assign(windowInstance, { SyntaxError });
    const previousGlobals = {
        document: globalThis.document,
        Node: globalThis.Node,
        Element: globalThis.Element,
        HTMLElement: globalThis.HTMLElement,
        HTMLInputElement: globalThis.HTMLInputElement,
        HTMLTextAreaElement: globalThis.HTMLTextAreaElement,
        HTMLSelectElement: globalThis.HTMLSelectElement,
        DOMParser: globalThis.DOMParser,
        SyntaxError: globalThis.SyntaxError,
    };
    Object.assign(globalThis, {
        document: windowInstance.document,
        Node: windowInstance.Node,
        Element: windowInstance.Element,
        HTMLElement: windowInstance.HTMLElement,
        HTMLInputElement: windowInstance.HTMLInputElement,
        HTMLTextAreaElement: windowInstance.HTMLTextAreaElement,
        HTMLSelectElement: windowInstance.HTMLSelectElement,
        DOMParser: windowInstance.DOMParser,
        SyntaxError,
    });
    try {
        document.body.innerHTML = await render();
        for (let iteration = 0; iteration < 3; iteration += 1) {
            const replace = /** @type {Element} */ (document.querySelector('.replace'));
            await dispatchAndPatch(document, render, replace, 'click', new windowInstance.MouseEvent('click', { bubbles: true }));
        }
        const filter = /** @type {Element} */ (document.querySelector('.filter'));
        await dispatchAndPatch(document, render, filter, 'click', new windowInstance.MouseEvent('click', { bubbles: true }));

        const rowB = /** @type {Element} */ (document.querySelector('.row[data-row="b"]'));
        await dispatchAndPatch(document, render, rowB, 'click', new windowInstance.MouseEvent('click', { bubbles: true }));
        const archiveC = /** @type {Element} */ (document.querySelector('.archive[data-row="c"]'));
        await dispatchAndPatch(document, render, archiveC, 'click', new windowInstance.MouseEvent('click', { bubbles: true }));
        const currentRowC = /** @type {Element} */ (document.querySelector('.row[data-row="c"]'));
        await dispatchAndPatch(document, render, currentRowC, 'keydown', new windowInstance.KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));

        expect(document.querySelector('output')?.textContent).toBe('select:b|archive:c|key:c');
    }
    finally {
        Object.assign(globalThis, previousGlobals);
        await windowInstance.happyDOM.close();
    }
});

test('a handler on one repeated component descendant dispatches only that loop occurrence (#116)', async () => {
    const root = await bundleFixture();
    const moduleURL = `${pathToFileURL(webDistPath(root, 'pages', 'tac.js')).href}?issue-116-components=${Date.now()}`;
    const pageModule = await import(moduleURL);
    const render = await pageModule.default();
    const windowInstance = new Window({ url: 'http://localhost/' });
    Object.assign(windowInstance, { SyntaxError });
    const previousGlobals = {
        document: globalThis.document,
        Node: globalThis.Node,
        Element: globalThis.Element,
        HTMLElement: globalThis.HTMLElement,
        HTMLInputElement: globalThis.HTMLInputElement,
        HTMLTextAreaElement: globalThis.HTMLTextAreaElement,
        HTMLSelectElement: globalThis.HTMLSelectElement,
        DOMParser: globalThis.DOMParser,
        SyntaxError: globalThis.SyntaxError,
    };
    Object.assign(globalThis, {
        document: windowInstance.document,
        Node: windowInstance.Node,
        Element: windowInstance.Element,
        HTMLElement: windowInstance.HTMLElement,
        HTMLInputElement: windowInstance.HTMLInputElement,
        HTMLTextAreaElement: windowInstance.HTMLTextAreaElement,
        HTMLSelectElement: windowInstance.HTMLSelectElement,
        DOMParser: windowInstance.DOMParser,
        SyntaxError,
        __issue116Hits: [],
    });
    try {
        document.body.innerHTML = await render();
        for (let iteration = 0; iteration < 3; iteration += 1) {
            const replace = /** @type {Element} */ (document.querySelector('.replace'));
            await dispatchAndPatch(document, render, replace, 'click', new windowInstance.MouseEvent('click', { bubbles: true }));
        }
        const filter = /** @type {Element} */ (document.querySelector('.filter'));
        await dispatchAndPatch(document, render, filter, 'click', new windowInstance.MouseEvent('click', { bubbles: true }));

        const rowB = /** @type {Element} */ (document.querySelector('.component-row[data-row="b"]'));
        await dispatchAndPatch(document, render, rowB, 'click', new windowInstance.MouseEvent('click', { bubbles: true }));
        const archiveC = /** @type {Element} */ (document.querySelector('.component-archive[data-row="c"]'));
        await dispatchAndPatch(document, render, archiveC, 'click', new windowInstance.MouseEvent('click', { bubbles: true }));
        const currentRowC = /** @type {Element} */ (document.querySelector('.component-row[data-row="c"]'));
        await dispatchAndPatch(document, render, currentRowC, 'keydown', new windowInstance.KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));

        expect(globalThis.__issue116Hits).toEqual(['select:b', 'archive:c', 'key:c']);
    }
    finally {
        Object.assign(globalThis, previousGlobals);
        Reflect.deleteProperty(globalThis, '__issue116Hits');
        await windowInstance.happyDOM.close();
    }
});
