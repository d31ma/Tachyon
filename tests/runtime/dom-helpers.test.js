// @ts-check
import { afterAll, beforeAll, describe, expect, test } from 'bun:test';
import { Window } from 'happy-dom';
import { cleanBooleanAttrs, findEventTarget, morphChildren, parseFragment, parseParams, resolveHandler, } from '../../src/runtime/dom-helpers.js';
/** @type {Record<string, unknown>} */
let previousGlobals;
/** @type {Window} */
let windowInstance;
beforeAll(() => {
    windowInstance = new Window();
    previousGlobals = {
        window: globalThis.window,
        document: globalThis.document,
        DOMParser: globalThis.DOMParser,
        Node: globalThis.Node,
        Element: globalThis.Element,
        DocumentFragment: globalThis.DocumentFragment,
        HTMLInputElement: globalThis.HTMLInputElement,
        HTMLTextAreaElement: globalThis.HTMLTextAreaElement,
        HTMLSelectElement: globalThis.HTMLSelectElement,
        SyntaxError: globalThis.SyntaxError,
    };
    Object.assign(windowInstance, {
        SyntaxError,
    });
    Object.assign(globalThis, {
        window: windowInstance,
        document: windowInstance.document,
        DOMParser: windowInstance.DOMParser,
        Node: windowInstance.Node,
        Element: windowInstance.Element,
        DocumentFragment: windowInstance.DocumentFragment,
        HTMLInputElement: windowInstance.HTMLInputElement,
        HTMLTextAreaElement: windowInstance.HTMLTextAreaElement,
        HTMLSelectElement: windowInstance.HTMLSelectElement,
        SyntaxError,
    });
});
afterAll(async () => {
    await windowInstance.happyDOM.close();
    Object.assign(globalThis, previousGlobals);
});
describe('findEventTarget', () => {
    test('finds the closest declarative event target', () => {
        document.body.innerHTML = `
          <div @click="outer()">
            <button id="child"><span id="inner">Tap</span></button>
          </div>
        `;
        const target = findEventTarget(/** @type {Element} */ (document.getElementById('inner')), 'click');
        expect(target?.getAttribute('@click')).toBe('outer()');
    });
});
describe('cleanBooleanAttrs', () => {
    test('removes false boolean-like attributes and keeps true values', () => {
        document.body.innerHTML = `
          <button id="a" disabled="false"></button>
          <button id="b" selected="false"></button>
          <button id="c" checked="true"></button>
        `;
        cleanBooleanAttrs();
        expect(document.getElementById('a')?.hasAttribute('disabled')).toBe(false);
        expect(document.getElementById('b')?.hasAttribute('selected')).toBe(false);
        expect(document.getElementById('c')?.getAttribute('checked')).toBe('true');
    });
});
describe('morphChildren', () => {
    test('updates keyed nodes in place and syncs attributes', () => {
        document.body.innerHTML = `<div id="root"><button id="save" class="old">Old</button></div>`;
        const root = /** @type {Element} */ (document.getElementById('root'));
        const original = document.getElementById('save');
        morphChildren(root, parseFragment(`<button id="save" class="new">New</button>`));
        const updated = document.getElementById('save');
        expect(updated).toBe(original);
        expect(updated?.className).toBe('new');
        expect(updated?.textContent).toBe('New');
    });
    test('syncs live input and textarea values after rerender', () => {
        document.body.innerHTML = `
          <div id="root">
            <input id="name" type="text" value="Draft" />
            <textarea id="note">Draft note</textarea>
          </div>
        `;
        const root = /** @type {Element} */ (document.getElementById('root'));
        const input = /** @type {HTMLInputElement} */ (document.getElementById('name'));
        const textarea = /** @type {HTMLTextAreaElement} */ (document.getElementById('note'));
        input.value = 'Local edit';
        textarea.value = 'Local note';
        morphChildren(root, parseFragment(`
          <input id="name" type="text" value="" />
          <textarea id="note"></textarea>
        `));
        expect(input.value).toBe('');
        expect(textarea.value).toBe('');
    });
    test('preserves lazy content when requested', () => {
        document.body.innerHTML = `<div id="root"><div id="lazy-shell"><strong>Loaded content</strong></div></div>`;
        const root = /** @type {Element} */ (document.getElementById('root'));
        morphChildren(root, parseFragment(`<div id="lazy-shell"></div>`), {
            preserveElement: (el) => el.id === 'lazy-shell'
        });
        expect(document.querySelector('#lazy-shell strong')?.textContent).toBe('Loaded content');
    });
});
describe('routing helpers', () => {
    test('resolves the best matching route and fills slugs', () => {
        /** @type {Map<string, Record<string, number | undefined>>} */
        const routes = new Map([
            ['/', {}],
            ['/docs', {}],
            ['/api/:version', { ':version': 1 }],
            ['/api/:version/users', { ':version': 1 }],
        ]);
        /** @type {Record<string, string>} */
        const slugs = {};
        const match = resolveHandler('/api/v2/users', /** @type {any} */ (routes), slugs);
        expect(match).toBe('/api/:version/users');
        expect(slugs.version).toBe('v2');
    });
    test('parses path params into typed values', () => {
        expect(parseParams(['42', 'true', 'null', 'hello'])).toEqual([42, true, null, 'hello']);
    });
});
