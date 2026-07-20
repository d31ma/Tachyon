// @ts-check
import { afterAll, afterEach, beforeAll, describe, expect, test } from 'bun:test';
import { Window } from 'happy-dom';
import { createComponentRegistry } from '../../src/runtime/component-registry.js';

/** @type {Record<string, unknown>} */
let previousGlobals;
/** @type {Window} */
let windowInstance;

beforeAll(() => {
    windowInstance = new Window({ url: 'http://localhost/' });
    previousGlobals = { window: globalThis.window, document: globalThis.document };
    Object.assign(globalThis, { window: windowInstance, document: windowInstance.document });
});
afterAll(async () => {
    await windowInstance.happyDOM.close();
    Object.assign(globalThis, previousGlobals);
});
afterEach(() => { document.body.innerHTML = ''; });

const noopRender = async () => '';

describe('component registry scoping decision', () => {
    test('a single-instance component at its canonical root is scopable', () => {
        const reg = createComponentRegistry();
        reg.register('tc-abc-0', noopRender, 'abc');
        expect(reg.scopable('tc-abc-0')).toEqual({ render: noopRender, compId: 'abc' });
    });

    test('an unregistered host is not scopable', () => {
        const reg = createComponentRegistry();
        expect(reg.scopable('tc-missing-0')).toBeNull();
    });

    test('a non-canonical host id (not tc-<compId>-0) is not scopable', () => {
        const reg = createComponentRegistry();
        // A host id whose suffix isn't -0 can't be the render root for its compId.
        reg.register('tc-abc-1', noopRender, 'abc');
        expect(reg.scopable('tc-abc-1')).toBeNull();
    });

    test('legacy/manual duplicate compIds across hosts fall back', () => {
        const reg = createComponentRegistry();
        reg.register('tc-abc-0', noopRender, 'abc');
        reg.register('tc-abc-1', noopRender, 'abc'); // Duplicate scope is ambiguous.
        // Even the canonical root is no longer scopable once the compId repeats.
        expect(reg.scopable('tc-abc-0')).toBeNull();
        expect(reg.scopable('tc-abc-1')).toBeNull();
    });

    test('ignores malformed registrations', () => {
        const reg = createComponentRegistry();
        reg.register('', noopRender, 'abc');
        reg.register('tc-x-0', /** @type {any} */ (null), 'x');
        expect(reg.scopable('tc-x-0')).toBeNull();
    });
});

describe('findAncestor walks to the nearest scopable component host', () => {
    test('returns the nearest registered component host above the trigger', () => {
        document.body.innerHTML = `
            <div id="tc-outer-0">
                <div id="tc-inner-0">
                    <button id="tc-btn-0">go</button>
                </div>
            </div>`;
        const reg = createComponentRegistry();
        reg.register('tc-outer-0', noopRender, 'outer');
        reg.register('tc-inner-0', noopRender, 'inner');
        const found = reg.findAncestor('tc-btn-0');
        expect(found?.host.id).toBe('tc-inner-0'); // nearest, not outer
    });

    test('skips non-scopable duplicate-id hosts and keeps walking up', () => {
        document.body.innerHTML = `
            <div id="tc-outer-0">
                <div id="tc-loop-1"><button id="tc-btn-0">go</button></div>
            </div>`;
        const reg = createComponentRegistry();
        reg.register('tc-outer-0', noopRender, 'outer');
        reg.register('tc-loop-0', noopRender, 'loop');
        reg.register('tc-loop-1', noopRender, 'loop'); // duplicate → not scopable
        const found = reg.findAncestor('tc-btn-0');
        expect(found?.host.id).toBe('tc-outer-0'); // falls through the ambiguous instance
    });

    test('returns null when no component ancestor exists (page-level trigger)', () => {
        document.body.innerHTML = `<button id="tc-btn-0">go</button>`;
        const reg = createComponentRegistry();
        expect(reg.findAncestor('tc-btn-0')).toBeNull();
        expect(reg.findAncestor(null)).toBeNull();
    });

    test('a handler ON a child component host scopes to the parent (handler owner)', () => {
        // `<child on:done="parentMethod()">` — the marker sits on the child host,
        // but the handler belongs to the parent's render.
        document.body.innerHTML = `
            <div id="tc-parent-0">
                <div id="tc-child-0">child content</div>
            </div>`;
        const reg = createComponentRegistry();
        reg.register('tc-parent-0', noopRender, 'parent');
        reg.register('tc-child-0', noopRender, 'child');
        // Trigger is the child host itself → owner is the parent, not the child.
        expect(reg.findAncestor('tc-child-0')?.host.id).toBe('tc-parent-0');
    });

    test('a handler on a regular element inside a component scopes to that component', () => {
        document.body.innerHTML = `
            <div id="tc-comp-0">
                <button id="tc-btn-0">go</button>
            </div>`;
        const reg = createComponentRegistry();
        reg.register('tc-comp-0', noopRender, 'comp');
        expect(reg.findAncestor('tc-btn-0')?.host.id).toBe('tc-comp-0');
    });
});
