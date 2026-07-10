// @ts-check
import { afterAll, afterEach, beforeAll, describe, expect, test } from 'bun:test';
import { Window } from 'happy-dom';
import { EVENT_CAPTURE_SCRIPT, createDeferredDelegation } from '../../src/runtime/event-hydration.js';

describe('createDeferredDelegation (lazy/intent registration + replay)', () => {
    /** @returns {{ queue: any[], onIntent: null | (() => void), stop: () => void, stopped: boolean }} */
    function fakeCapture(queue = []) {
        return { queue, onIntent: null, stop() { this.stopped = true; }, stopped: false };
    }

    test('defers registration to idle, then registers the event set', () => {
        /** @type {string[]} */
        const registered = [];
        /** @type {(() => void) | null} */
        let idle = null;
        const capture = fakeCapture();
        const d = createDeferredDelegation({
            ensure: (n) => registered.push(n),
            requestIdle: (cb) => { idle = cb; },
            getCapture: () => capture,
            dispatch: () => {},
        });
        d.schedule(['click', 'save']);
        expect(registered).toEqual([]); // nothing registered yet — off the critical path
        expect(typeof capture.onIntent).toBe('function'); // intent hook wired
        /** @type {any} */ (idle)(); // browser goes idle
        expect(registered).toEqual(['click', 'save']);
    });

    test('flushes immediately (skipping idle) when the user already interacted', () => {
        /** @type {string[]} */
        const registered = [];
        /** @type {string[]} */
        const replayed = [];
        let idleScheduled = false;
        const target = { isConnected: true };
        const capture = fakeCapture([{ type: 'click', target }]);
        const d = createDeferredDelegation({
            ensure: (n) => registered.push(n),
            requestIdle: () => { idleScheduled = true; },
            getCapture: () => capture,
            dispatch: (_t, type) => replayed.push(type),
        });
        d.schedule(['click']);
        expect(registered).toEqual(['click']);
        expect(idleScheduled).toBe(false);
        expect(replayed).toEqual(['click']); // captured interaction replayed
        expect(capture.stopped).toBe(true);
    });

    test('flushes the moment intent fires before idle', () => {
        /** @type {string[]} */
        const registered = [];
        const capture = fakeCapture();
        const d = createDeferredDelegation({
            ensure: (n) => registered.push(n),
            requestIdle: () => {}, // idle never comes
            getCapture: () => capture,
            dispatch: () => {},
        });
        d.schedule(['save']);
        expect(registered).toEqual([]);
        /** @type {any} */ (capture.onIntent)(); // user interacts
        expect(registered).toEqual(['save']);
    });

    test('registers late event sets immediately once already flushed', () => {
        /** @type {string[]} */
        const registered = [];
        /** @type {(() => void) | null} */
        let idle = null;
        const capture = fakeCapture();
        const d = createDeferredDelegation({
            ensure: (n) => registered.push(n),
            requestIdle: (cb) => { idle = cb; },
            getCapture: () => capture,
            dispatch: () => {},
        });
        d.schedule(['click']);
        /** @type {any} */ (idle)();
        expect(registered).toEqual(['click']);
        d.schedule(['save']); // a lazily-loaded component registers after hydration
        expect(registered).toEqual(['click', 'save']);
    });

    test('replay skips targets removed by the hydration morph, replays once', () => {
        /** @type {any[]} */
        const replayed = [];
        const capture = fakeCapture([
            { type: 'click', target: { isConnected: false } },
            { type: 'submit', target: { isConnected: true } },
        ]);
        const d = createDeferredDelegation({
            ensure: () => {},
            requestIdle: (cb) => cb(),
            getCapture: () => capture,
            dispatch: (t, type) => replayed.push(type),
        });
        d.schedule(['click']);
        expect(replayed).toEqual(['submit']); // disconnected target dropped
        // A second flush (late component) must not replay again.
        d.schedule(['x']);
        expect(replayed).toEqual(['submit']);
    });
});

describe('EVENT_CAPTURE_SCRIPT (pre-hydration dead-zone capture)', () => {
    /** @type {Record<string, unknown>} */
    let previousGlobals;
    /** @type {Window} */
    let windowInstance;

    beforeAll(() => {
        windowInstance = new Window({ url: 'http://localhost/' });
        previousGlobals = {
            window: globalThis.window,
            document: globalThis.document,
            Event: globalThis.Event,
        };
        Object.assign(globalThis, {
            window: windowInstance,
            document: windowInstance.document,
            Event: windowInstance.Event,
        });
        // eslint-disable-next-line no-eval
        (0, eval)(EVENT_CAPTURE_SCRIPT);
    });
    afterAll(async () => {
        capture()?.stop();
        await windowInstance.happyDOM.close();
        Object.assign(globalThis, previousGlobals);
    });
    afterEach(() => {
        capture().queue.length = 0;
        document.body.innerHTML = '';
    });

    // The inline script assigns to `window.__tacEventCapture`; `window` is the
    // happy-dom instance, which is not the same object as globalThis here.
    /** @returns {any} */
    const capture = () => /** @type {any} */ (windowInstance).__tacEventCapture;

    test('records and neutralizes a click on a Tac handler', () => {
        document.body.innerHTML = `<button id="b" data-tac-on-click="">go</button>`;
        const button = /** @type {Element} */ (document.getElementById('b'));
        const event = new windowInstance.Event('click', { bubbles: true, cancelable: true });
        button.dispatchEvent(/** @type {any} */ (event));
        expect(capture().queue).toEqual([{ type: 'click', target: button }]);
        expect(event.defaultPrevented).toBe(true);
    });

    test('records a click on an internal anchor (for SPA-nav replay)', () => {
        document.body.innerHTML = `<a id="a" href="/about"><span id="inner">About</span></a>`;
        const inner = /** @type {Element} */ (document.getElementById('inner'));
        inner.dispatchEvent(/** @type {any} */ (new windowInstance.Event('click', { bubbles: true, cancelable: true })));
        expect(capture().queue).toEqual([{ type: 'click', target: document.getElementById('a') }]);
    });

    test('records a click on an href-bearing web component (for SPA-nav replay)', () => {
        document.body.innerHTML = `<w-btn id="docs" href="/docs">Docs</w-btn>`;
        const button = /** @type {Element} */ (document.getElementById('docs'));
        button.dispatchEvent(/** @type {any} */ (new windowInstance.Event('click', { bubbles: true, cancelable: true })));
        expect(capture().queue).toEqual([{ type: 'click', target: button }]);
    });

    test('ignores clicks that do not target a Tac handler', () => {
        document.body.innerHTML = `<div id="plain">nothing</div>`;
        const plain = /** @type {Element} */ (document.getElementById('plain'));
        const event = new windowInstance.Event('click', { bubbles: true, cancelable: true });
        plain.dispatchEvent(/** @type {any} */ (event));
        expect(capture().queue).toEqual([]);
        expect(event.defaultPrevented).toBe(false);
    });

    test('notifies the runtime intent hook when an interaction is captured', () => {
        document.body.innerHTML = `<button id="b" data-tac-on-save="">save</button>`;
        let intent = 0;
        capture().onIntent = () => { intent += 1; };
        document.getElementById('b')?.dispatchEvent(/** @type {any} */ (new windowInstance.Event('click', { bubbles: true, cancelable: true })));
        expect(intent).toBe(1);
        capture().onIntent = null;
    });
});
