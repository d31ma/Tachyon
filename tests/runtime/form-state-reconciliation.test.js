// @ts-check
import { afterAll, beforeAll, beforeEach, describe, expect, test } from 'bun:test';
import { Window } from 'happy-dom';
import { createFocusLease, morphChildren, parseFragment } from '../../src/runtime/dom-helpers.js';

/** @type {Window} */
let windowInstance;
/** @type {Record<string, unknown>} */
let previousGlobals;

beforeAll(() => {
    windowInstance = new Window({ url: 'http://localhost/' });
    Object.assign(windowInstance, { SyntaxError });
    previousGlobals = {
        window: globalThis.window,
        document: globalThis.document,
        Node: globalThis.Node,
        Element: globalThis.Element,
        HTMLElement: globalThis.HTMLElement,
        HTMLInputElement: globalThis.HTMLInputElement,
        HTMLTextAreaElement: globalThis.HTMLTextAreaElement,
        HTMLSelectElement: globalThis.HTMLSelectElement,
        DOMParser: globalThis.DOMParser,
    };
    Object.assign(globalThis, {
        window: windowInstance,
        document: windowInstance.document,
        Node: windowInstance.Node,
        Element: windowInstance.Element,
        HTMLElement: windowInstance.HTMLElement,
        HTMLInputElement: windowInstance.HTMLInputElement,
        HTMLTextAreaElement: windowInstance.HTMLTextAreaElement,
        HTMLSelectElement: windowInstance.HTMLSelectElement,
        DOMParser: windowInstance.DOMParser,
    });
});

afterAll(async () => {
    await windowInstance.happyDOM.close();
    Object.assign(globalThis, previousGlobals);
});

beforeEach(() => {
    document.body.innerHTML = '';
});

describe('reactive form reconciliation (#115)', () => {
    test('preserves the active input value and selection when rendered state is stale', () => {
        document.body.innerHTML = `
          <main id="compose">
            <input id="to" value="bob@example.com" />
            <input id="subject" value="" />
          </main>`;
        const compose = /** @type {HTMLElement} */ (document.getElementById('compose'));
        const subject = /** @type {HTMLInputElement} */ (document.getElementById('subject'));
        subject.focus();
        subject.value = 'Test email';
        subject.setSelectionRange(4, 8, 'forward');

        morphChildren(compose, parseFragment(`
          <input id="to" value="bob@example.com" />
          <input id="subject" value="" />`));

        expect(document.activeElement).toBe(subject);
        expect(subject.value).toBe('Test email');
        expect(subject.selectionStart).toBe(4);
        expect(subject.selectionEnd).toBe(8);
        expect(subject.selectionDirection).toBe('forward');
    });

    test('preserves active contenteditable contents and caret across a stale rerender', () => {
        document.body.innerHTML = `
          <main id="compose">
            <div id="body" contenteditable="true">Draft body</div>
          </main>`;
        const compose = /** @type {HTMLElement} */ (document.getElementById('compose'));
        const editor = /** @type {HTMLElement} */ (document.getElementById('body'));
        editor.focus();
        const selection = document.getSelection();
        const range = document.createRange();
        range.setStart(/** @type {Text} */ (editor.firstChild), 5);
        range.collapse(true);
        selection?.removeAllRanges();
        selection?.addRange(range);

        morphChildren(compose, parseFragment(`
          <div id="body" contenteditable="true"></div>`));

        expect(document.activeElement).toBe(editor);
        expect(editor.textContent).toBe('Draft body');
        expect(document.getSelection()?.anchorNode).toBe(editor.firstChild);
        expect(document.getSelection()?.anchorOffset).toBe(5);
    });

    test('a pending rerender does not steal focus after the user moves to an action', () => {
        document.body.innerHTML = `
          <main id="compose">
            <input id="subject" value="Test email" />
            <button id="send">Send</button>
          </main>`;
        const compose = /** @type {HTMLElement} */ (document.getElementById('compose'));
        const subject = /** @type {HTMLInputElement} */ (document.getElementById('subject'));
        const send = /** @type {HTMLButtonElement} */ (document.getElementById('send'));
        subject.focus();
        const focusLease = createFocusLease('subject');
        send.focus();

        morphChildren(compose, parseFragment(`
          <input id="subject" value="Test email" />
          <button id="send">Send</button>`));
        focusLease.restore();

        expect(document.activeElement).toBe(send);
    });

    test('a pending rerender restores focus only when its active trigger was replaced', () => {
        document.body.innerHTML = '<input id="subject" data-tac-id="tc-subject-0" value="Draft" />';
        const subject = /** @type {HTMLInputElement} */ (document.getElementById('subject'));
        subject.focus();
        const focusLease = createFocusLease('tc-subject-0');

        const replacement = /** @type {HTMLInputElement} */ (subject.cloneNode(true));
        subject.replaceWith(replacement);
        expect(document.activeElement).toBe(document.body);
        focusLease.restore();

        expect(document.activeElement).toBe(replacement);
    });

    test('carries live value and selection to a replacement caused by sibling insertion', () => {
        document.body.innerHTML = '<main id="compose"><input id="subject" value="" /></main>';
        const compose = /** @type {HTMLElement} */ (document.getElementById('compose'));
        const subject = /** @type {HTMLInputElement} */ (document.getElementById('subject'));
        subject.focus();
        subject.value = 'Test email';
        subject.setSelectionRange(4, 8, 'forward');
        const focusLease = createFocusLease('subject');

        morphChildren(compose, parseFragment(`
          <p id="hint">Ready</p>
          <input id="subject" value="" />`));
        focusLease.restore();

        const replacement = /** @type {HTMLInputElement} */ (document.getElementById('subject'));
        expect(replacement).not.toBe(subject);
        expect(document.activeElement).toBe(replacement);
        expect(replacement.value).toBe('Test email');
        expect(replacement.selectionStart).toBe(4);
        expect(replacement.selectionEnd).toBe(8);
        expect(replacement.selectionDirection).toBe('forward');
    });

    test('does not carry contenteditable state onto a same-id non-editor replacement', () => {
        document.body.innerHTML = '<main id="compose"><div id="body" contenteditable="true">Draft body</div></main>';
        const compose = /** @type {HTMLElement} */ (document.getElementById('compose'));
        const editor = /** @type {HTMLElement} */ (document.getElementById('body'));
        editor.focus();
        const focusLease = createFocusLease('body');

        morphChildren(compose, parseFragment('<button id="body">Submit</button>'));
        focusLease.restore();

        expect(document.getElementById('body')?.textContent).toBe('Submit');
    });

    test('preserves a backward contenteditable selection on replacement', () => {
        document.body.innerHTML = '<main id="compose"><div id="body" contenteditable="true">Draft body</div></main>';
        const compose = /** @type {HTMLElement} */ (document.getElementById('compose'));
        const editor = /** @type {HTMLElement} */ (document.getElementById('body'));
        const text = /** @type {Text} */ (editor.firstChild);
        editor.focus();
        document.getSelection()?.setBaseAndExtent(text, 8, text, 2);
        const focusLease = createFocusLease('body');

        morphChildren(compose, parseFragment(`
          <p id="hint">Editing</p>
          <div id="body" contenteditable="true"></div>`));
        focusLease.restore();

        const selection = document.getSelection();
        expect(selection?.anchorOffset).toBe(8);
        expect(selection?.focusOffset).toBe(2);
    });

    test('carries every live selection to a replacement multiple-select', () => {
        document.body.innerHTML = `
          <main id="compose"><select id="tags" multiple>
            <option>Alpha</option><option>Beta</option><option>Gamma</option>
          </select></main>`;
        const compose = /** @type {HTMLElement} */ (document.getElementById('compose'));
        const tags = /** @type {HTMLSelectElement} */ (document.getElementById('tags'));
        tags.options[0].selected = true;
        tags.options[2].selected = true;
        tags.focus();
        const focusLease = createFocusLease('tags');

        morphChildren(compose, parseFragment(`
          <p id="hint">Choose tags</p>
          <select id="tags" multiple>
            <option>Alpha</option><option>Beta</option><option>Gamma</option>
          </select>`));
        focusLease.restore();

        const replacement = /** @type {HTMLSelectElement} */ (document.getElementById('tags'));
        expect(Array.from(replacement.options).map((option) => option.selected)).toEqual([true, false, true]);
    });

    test('carries selected values when replacement options reorder', () => {
        document.body.innerHTML = `
          <main id="compose"><select id="priority">
            <option value="alpha">Alpha</option><option value="beta">Beta</option>
          </select></main>`;
        const compose = /** @type {HTMLElement} */ (document.getElementById('compose'));
        const priority = /** @type {HTMLSelectElement} */ (document.getElementById('priority'));
        priority.value = 'beta';
        priority.focus();
        const focusLease = createFocusLease('priority');

        morphChildren(compose, parseFragment(`
          <p id="hint">Priority</p>
          <select id="priority">
            <option value="beta">Beta</option><option value="alpha">Alpha</option>
          </select>`));
        focusLease.restore();

        expect(/** @type {HTMLSelectElement} */ (document.getElementById('priority')).value).toBe('beta');
    });

    test('does not restore the old trigger when the newly focused action is removed by the patch', () => {
        document.body.innerHTML = `
          <input id="subject" value="Draft" />
          <button id="send">Send</button>`;
        const subject = /** @type {HTMLInputElement} */ (document.getElementById('subject'));
        const send = /** @type {HTMLButtonElement} */ (document.getElementById('send'));
        subject.focus();
        const focusLease = createFocusLease('subject');

        send.focus();
        send.remove();
        expect(document.activeElement).toBe(document.body);
        focusLease.restore();

        expect(document.activeElement).toBe(document.body);
    });

    test('does not transfer a secret through an authored-id identity collision', () => {
        document.body.innerHTML = `
          <main id="compose">
            <input id="credential" data-tac-id="tc-password-old" type="password" value="" />
          </main>`;
        const compose = /** @type {HTMLElement} */ (document.getElementById('compose'));
        const password = /** @type {HTMLInputElement} */ (document.getElementById('credential'));
        password.value = 'correct horse battery staple';
        password.focus();
        const focusLease = createFocusLease('tc-password-old');

        morphChildren(compose, parseFragment(`
          <input id="credential" data-tac-id="tc-text-new" type="text" value="" />`));
        focusLease.restore();

        const replacement = /** @type {HTMLInputElement} */ (document.getElementById('credential'));
        expect(document.activeElement).toBe(document.body);
        expect(replacement.value).toBe('');
    });

    test('treats one-sided compiler identity as a different control', () => {
        document.body.innerHTML = `
          <main id="compose">
            <input id="credential" data-tac-id="tc-password-old" type="password" value="" />
          </main>`;
        const compose = /** @type {HTMLElement} */ (document.getElementById('compose'));
        const password = /** @type {HTMLInputElement} */ (document.getElementById('credential'));
        password.value = 'secret';
        password.focus();
        const focusLease = createFocusLease('tc-password-old');

        morphChildren(compose, parseFragment('<input id="credential" type="text" value="" />'));
        focusLease.restore();

        const replacement = /** @type {HTMLInputElement} */ (document.getElementById('credential'));
        expect(replacement).not.toBe(password);
        expect(document.activeElement).toBe(document.body);
        expect(replacement.value).toBe('');
    });

    test('does not cross from authored identity into a compiler-owned control', () => {
        document.body.innerHTML = `
          <main id="compose"><input id="credential" type="password" value="" /></main>`;
        const compose = /** @type {HTMLElement} */ (document.getElementById('compose'));
        const password = /** @type {HTMLInputElement} */ (document.getElementById('credential'));
        password.value = 'secret';
        password.focus();
        const focusLease = createFocusLease('credential');

        morphChildren(compose, parseFragment(`
          <input id="credential" data-tac-id="tc-password-new" type="password" value="" />`));
        focusLease.restore();

        const replacement = /** @type {HTMLInputElement} */ (document.getElementById('credential'));
        expect(replacement).not.toBe(password);
        expect(document.activeElement).toBe(document.body);
        expect(replacement.value).toBe('');
    });
});
