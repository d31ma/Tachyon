// @ts-check
import { afterAll, beforeAll, describe, expect, test } from 'bun:test';
import { Window } from 'happy-dom';
import { cleanBooleanAttrs, createValueEventDetail, findEventTarget, morphChildren, parseFragment, parseParams, repointCurrentTarget, resolveHandler, } from '../../src/runtime/dom-helpers.js';
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
        HTMLElement: globalThis.HTMLElement,
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
        HTMLElement: windowInstance.HTMLElement,
        SyntaxError,
    });
});
afterAll(async () => {
    await windowInstance.happyDOM.close();
    Object.assign(globalThis, previousGlobals);
});
describe('findEventTarget', () => {
    test('finds the closest declarative event target by walking up to the marker', () => {
        document.body.innerHTML = `
          <div id="host" data-tac-on-click="">
            <button id="child"><span id="inner">Tap</span></button>
          </div>
        `;
        const target = findEventTarget(/** @type {Element} */ (document.getElementById('inner')), 'click');
        expect(target?.id).toBe('host');
    });
    test('finds the closest target via the compiled data-tac-on-<event> marker', () => {
        document.body.innerHTML = `
          <div id="host" data-tac-on-save="">
            <w-confirm-edit><button id="inner">Save</button></w-confirm-edit>
          </div>
        `;
        const target = findEventTarget(/** @type {Element} */ (document.getElementById('inner')), 'save');
        expect(target?.id).toBe('host');
    });
    test('matches a Vue-style colon event via the encoded marker', () => {
        document.body.innerHTML = `<w-data-table id="host" data-tac-on-update__selected=""></w-data-table>`;
        const target = findEventTarget(/** @type {Element} */ (document.getElementById('host')), 'update:selected');
        expect(target?.id).toBe('host');
    });
    test('creates DOM-compatible context for synthetic value rerenders', () => {
        const input = document.createElement('input');
        input.value = 'selected';
        const detail = createValueEventDetail(input, /** @type {any} */ (new windowInstance.Event('input')));
        expect(detail.value).toBe('selected');
        expect(detail.target).toBe(input);
        expect(detail.currentTarget).toBe(input);
        expect(detail.type).toBe('input');
    });
    test('repointCurrentTarget overrides a native event currentTarget the browser reset (#110 comment)', () => {
        document.body.innerHTML = `<div id="pane" data-tac-on-keydown=""><input id="field" /></div>`;
        const pane = /** @type {Element} */ (document.getElementById('pane'));
        // A finished native event reads currentTarget back as null (or document),
        // never the delegated handler's element.
        const event = new windowInstance.Event('keydown');
        expect(event.currentTarget).not.toBe(pane);
        repointCurrentTarget(event, pane);
        expect(event.currentTarget).toBe(pane);
        // @ts-expect-error runtime check: closest() is now reachable off currentTarget
        expect(event.currentTarget.closest('#pane')).toBe(pane);
    });
    test('repointCurrentTarget no-ops on a frozen event without throwing', () => {
        const frozen = Object.freeze({ type: 'click', target: null, currentTarget: null });
        expect(() => repointCurrentTarget(frozen, /** @type {any} */ ({}))).not.toThrow();
        expect(frozen.currentTarget).toBeNull();
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
    test('updates authored slots without dismantling a Light DOM web component', () => {
        class LightCard extends HTMLElement {
            connectedCallback() {
                if (this.querySelector('.light-card-shell'))
                    return;
                const authoredContent = Array.from(this.childNodes);
                const slot = document.createElement('slot');
                slot.append(...authoredContent);
                const shell = document.createElement('section');
                shell.className = 'light-card-shell';
                shell.append(slot);
                this.replaceChildren(shell);
            }
        }
        windowInstance.customElements.define('test-light-card', LightCard);
        document.body.innerHTML = `
          <div id="root">
            <test-light-card id="card" tone="quiet">Initial label</test-light-card>
          </div>
        `;
        const root = /** @type {Element} */ (document.getElementById('root'));
        const original = document.getElementById('card');
        morphChildren(root, parseFragment(`
          <test-light-card id="card" tone="loud">Updated label</test-light-card>
        `));
        const updated = document.getElementById('card');
        expect(updated).toBe(original);
        expect(updated?.getAttribute('tone')).toBe('loud');
        expect(updated?.querySelector('.light-card-shell')).not.toBeNull();
        expect(updated?.querySelector('slot')?.textContent).toBe('Updated label');
    });
    test('preserves a slotless Light DOM web component across rerenders', () => {
        class LightField extends HTMLElement {
            connectedCallback() {
                if (this.querySelector('.w-text-field'))
                    return;
                const wrapper = document.createElement('div');
                wrapper.className = 'w-text-field';
                wrapper.append(document.createElement('input'));
                this.replaceChildren(wrapper);
            }
        }
        windowInstance.customElements.define('test-light-field', LightField);
        document.body.innerHTML = `
          <div id="root">
            <test-light-field id="field" label="Email"></test-light-field>
          </div>
        `;
        const root = /** @type {Element} */ (document.getElementById('root'));
        morphChildren(root, parseFragment(`
          <test-light-field id="field" label="Name"></test-light-field>
        `));
        const updated = document.getElementById('field');
        expect(updated?.getAttribute('label')).toBe('Name');
        expect(updated?.querySelector('.w-text-field input')).not.toBeNull();
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
