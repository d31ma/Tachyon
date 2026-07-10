// @ts-check
import { afterAll, afterEach, beforeAll, expect, test } from 'bun:test';
import { Window } from 'happy-dom';
import { findEventTarget, findNavigationTarget } from '../../src/runtime/dom-helpers.js';

/**
 * These tests prove Tac interoperates with Light-DOM web components (DuVay) via
 * plain bubbling/composed CustomEvents: the compiled `data-tac-on-*` marker is
 * what delegation keys off, the raw `on:<event>` attribute is gone from the DOM, and
 * handlers still fire. The harness below mirrors spa-renderer's
 * handleDelegatedEvent (document listener → findEventTarget → run handler) using
 * the real findEventTarget so we exercise the shipped lookup logic.
 */

/** @type {Record<string, unknown>} */
let previousGlobals;
/** @type {Window} */
let windowInstance;
/** @type {(() => void)[]} */
const teardowns = [];

beforeAll(() => {
    windowInstance = new Window({ url: 'http://localhost/' });
    previousGlobals = {
        window: globalThis.window,
        document: globalThis.document,
        Node: globalThis.Node,
        Element: globalThis.Element,
        Event: globalThis.Event,
        CustomEvent: globalThis.CustomEvent,
        SyntaxError: globalThis.SyntaxError,
    };
    Object.assign(windowInstance, { SyntaxError });
    Object.assign(globalThis, {
        window: windowInstance,
        document: windowInstance.document,
        Node: windowInstance.Node,
        Element: windowInstance.Element,
        Event: windowInstance.Event,
        CustomEvent: windowInstance.CustomEvent,
        SyntaxError,
    });
});
afterAll(async () => {
    await windowInstance.happyDOM.close();
    Object.assign(globalThis, previousGlobals);
});
afterEach(() => {
    for (const teardown of teardowns.splice(0))
        teardown();
    document.body.innerHTML = '';
});

/**
 * Wire one delegated listener the way spa-renderer does: listen on document,
 * walk up from event.target via findEventTarget, run the recorded handler.
 * @param {string} eventName
 * @returns {{ hits: Element[] }}
 */
function delegate(eventName) {
    const hits = { hits: /** @type {Element[]} */ ([]) };
    /** @param {Event} event */
    const listener = (event) => {
        const target = event.target instanceof Element ? event.target : null;
        const handlerEl = findEventTarget(target, eventName);
        if (handlerEl)
            hits.hits.push(handlerEl);
    };
    document.addEventListener(eventName, listener);
    teardowns.push(() => document.removeEventListener(eventName, listener));
    return hits;
}

test('a simple custom event fires the marker handler, and no on:<event> attr is left in the DOM', () => {
    document.body.innerHTML = `<w-confirm-edit id="host" data-tac-on-save="">edit</w-confirm-edit>`;
    const host = /** @type {Element} */ (document.getElementById('host'));
    const saved = delegate('save');
    // DuVay-style dispatch from the host element.
    host.dispatchEvent(new CustomEvent('save', { detail: { id: 1 }, bubbles: true, composed: true }));
    expect(saved.hits).toEqual([host]);
    expect(host.hasAttribute('on:save')).toBe(false);
    expect(host.hasAttribute('data-tac-on-save')).toBe(true);
});

test('a Vue-style colon event (update:selected) delegates via the encoded marker', () => {
    document.body.innerHTML = `<w-data-table id="host" data-tac-on-update__selected="">table</w-data-table>`;
    const host = /** @type {Element} */ (document.getElementById('host'));
    const picked = delegate('update:selected');
    host.dispatchEvent(new CustomEvent('update:selected', { detail: [3], bubbles: true, composed: true }));
    expect(picked.hits).toEqual([host]);
    expect(host.hasAttribute('on:update:selected')).toBe(false);
});

test('a bubbling+composed event from a nested Light-DOM custom element reaches the delegated handler', () => {
    document.body.innerHTML = `
        <div id="host" data-tac-on-load="">
            <w-infinite-scroll id="scroller">
                <button id="sentinel">more</button>
            </w-infinite-scroll>
        </div>`;
    const host = /** @type {Element} */ (document.getElementById('host'));
    const sentinel = /** @type {Element} */ (document.getElementById('sentinel'));
    const loaded = delegate('load');
    // Event originates deep inside nested custom elements and bubbles up.
    sentinel.dispatchEvent(new CustomEvent('load', { bubbles: true, composed: true }));
    expect(loaded.hits).toEqual([host]);
});

test('arbitrary custom event names delegate the same way (no allow-list)', () => {
    document.body.innerHTML = `<w-thing id="host" data-tac-on-clear="">x</w-thing>`;
    const host = /** @type {Element} */ (document.getElementById('host'));
    const cleared = delegate('clear');
    host.dispatchEvent(new CustomEvent('clear', { bubbles: true, composed: true }));
    expect(cleared.hits).toEqual([host]);
});

test('an href-bearing web component is a client-navigation target', () => {
    document.body.innerHTML = `<w-btn id="atlas" href="/atlas">Atlas</w-btn>`;
    const button = /** @type {Element} */ (document.getElementById('atlas'));
    const event = new windowInstance.Event('click', { bubbles: true, composed: true });
    button.dispatchEvent(/** @type {any} */ (event));
    expect(findNavigationTarget(event)).toBe(button);
});
