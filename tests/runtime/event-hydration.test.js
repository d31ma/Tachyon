// @ts-check
import { afterAll, afterEach, beforeAll, describe, expect, test } from 'bun:test';
import { Window } from 'happy-dom';
import { EVENT_CAPTURE_SCRIPT, createCapturedReplayEvent, createDeferredDelegation, resumeCapturedNativeAction } from '../../src/runtime/event-hydration.js';

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

    test('awaits island activation before registering and replaying the captured event', async () => {
        /** @type {string[]} */
        const order = [];
        const target = { isConnected: true };
        const capture = fakeCapture([{ type: 'click', target }]);
        const d = createDeferredDelegation({
            ensure: (type) => order.push(`register:${type}`),
            requestIdle: () => {},
            getCapture: () => capture,
            beforeReplay: async (candidate, type) => {
                expect(candidate).toBe(target);
                order.push(`hydrate:${type}`);
                await Promise.resolve();
            },
            dispatch: (_candidate, type) => order.push(`replay:${type}`),
        });
        d.schedule(['click']);
        await d.flush();
        expect(order).toEqual(['register:click', 'hydrate:click', 'replay:click']);
    });

    test('handles a failed replay record without rejecting flush or replaying it', async () => {
        const target = { isConnected: true };
        const capture = fakeCapture([{ type: 'click', target }]);
        /** @type {unknown[]} */
        const errors = [];
        /** @type {string[]} */
        const replayed = [];
        const d = createDeferredDelegation({
            ensure: () => {},
            requestIdle: () => {},
            getCapture: () => capture,
            beforeReplay: async () => { throw new Error('module unavailable'); },
            onReplayError: (error, record) => {
                errors.push(error);
                expect(record).toEqual({ type: 'click', target });
            },
            dispatch: (_target, type) => replayed.push(type),
        });
        d.schedule(['click']);
        await expect(d.flush()).resolves.toBeUndefined();
        expect(errors).toHaveLength(1);
        expect(replayed).toEqual([]);
        expect(capture.queue).toEqual([]);
        expect(capture.stopped).toBe(true);
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
            MutationObserver: globalThis.MutationObserver,
        };
        Object.assign(globalThis, {
            window: windowInstance,
            document: windowInstance.document,
            Event: windowInstance.Event,
            MutationObserver: windowInstance.MutationObserver,
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
        let laterCaptureRuns = 0;
        const laterCapture = () => { laterCaptureRuns += 1; };
        document.addEventListener('click', laterCapture, true);
        const event = new windowInstance.Event('click', { bubbles: true, cancelable: true });
        button.dispatchEvent(/** @type {any} */ (event));
        document.removeEventListener('click', laterCapture, true);
        expect(capture().queue).toHaveLength(1);
        expect(capture().queue[0]).toMatchObject({ type: 'click', target: button });
        expect(event.defaultPrevented).toBe(true);
        // The head recorder owns this native event. Later island/delegation
        // capture listeners must not queue or dispatch it a second time.
        expect(laterCaptureRuns).toBe(0);
    });

    test('records the first keyboard interaction for replay', () => {
        document.body.innerHTML = `<input id="search" data-tac-on-keydown="">`;
        const input = /** @type {Element} */ (document.getElementById('search'));
        input.dispatchEvent(/** @type {any} */ (new windowInstance.Event('keydown', { bubbles: true, cancelable: true })));
        expect(capture().queue).toHaveLength(1);
        expect(capture().queue[0]).toMatchObject({ type: 'keydown', target: input });
    });

    test('discovers and records custom compiled event markers', async () => {
        document.body.innerHTML = `<div id="picker" data-tac-on-update__selected=""></div>`;
        await Promise.resolve();
        const picker = /** @type {Element} */ (document.getElementById('picker'));
        picker.dispatchEvent(/** @type {any} */ (new windowInstance.Event('update:selected', { bubbles: true })));
        expect(capture().queue).toHaveLength(1);
        expect(capture().queue[0]).toMatchObject({ type: 'update:selected', target: picker });
    });

    test('records a click on an internal anchor (for SPA-nav replay)', () => {
        document.body.innerHTML = `<a id="a" href="/about"><span id="inner">About</span></a>`;
        const inner = /** @type {Element} */ (document.getElementById('inner'));
        inner.dispatchEvent(/** @type {any} */ (new windowInstance.Event('click', { bubbles: true, cancelable: true })));
        expect(capture().queue).toHaveLength(1);
        expect(capture().queue[0]).toMatchObject({ type: 'click', target: document.getElementById('a') });
    });

    test('records a click on an href-bearing web component (for SPA-nav replay)', () => {
        document.body.innerHTML = `<w-btn id="docs" href="/docs">Docs</w-btn>`;
        const button = /** @type {Element} */ (document.getElementById('docs'));
        button.dispatchEvent(/** @type {any} */ (new windowInstance.Event('click', { bubbles: true, cancelable: true })));
        expect(capture().queue).toHaveLength(1);
        expect(capture().queue[0]).toMatchObject({ type: 'click', target: button });
    });

    test('preserves modified non-primary click fields for replay and SPA eligibility', () => {
        document.body.innerHTML = '<a id="docs" href="/docs">Docs</a>';
        const anchor = /** @type {Element} */ (document.getElementById('docs'));
        anchor.dispatchEvent(new windowInstance.MouseEvent('click', {
            bubbles: true,
            cancelable: true,
            ctrlKey: true,
            shiftKey: true,
            button: 1,
            buttons: 4,
            clientX: 42,
        }));
        const record = capture().queue[0];
        const replay = /** @type {MouseEvent} */ (createCapturedReplayEvent(anchor, record));
        expect(replay.ctrlKey).toBe(true);
        expect(replay.shiftKey).toBe(true);
        expect(replay.button).toBe(1);
        expect(replay.buttons).toBe(4);
        expect(replay.clientX).toBe(42);
        const spaEligible = !replay.ctrlKey && !replay.metaKey
            && !replay.shiftKey && !replay.altKey && replay.button === 0;
        expect(spaEligible).toBe(false);
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
        let intentRecord;
        capture().onIntent = (record) => { intent += 1; intentRecord = record; };
        document.getElementById('b')?.dispatchEvent(/** @type {any} */ (new windowInstance.Event('click', { bubbles: true, cancelable: true })));
        expect(intent).toBe(1);
        expect(intentRecord).toBe(capture().queue[0]);
        capture().onIntent = null;
    });
});

describe('resumeCapturedNativeAction', () => {
    test('navigates a captured anchor when island activation fails', () => {
        const windowInstance = new Window({ url: 'http://localhost/' });
        windowInstance.document.body.innerHTML = '<a id="docs" href="/docs">Docs</a>';
        /** @type {string[]} */
        const navigations = [];
        const resumed = resumeCapturedNativeAction(
            /** @type {Element} */ (windowInstance.document.getElementById('docs')),
            'click',
            { navigate: (href) => navigations.push(href) },
        );
        expect(resumed).toBe(true);
        expect(navigations).toEqual(['/docs']);
    });

    test.each(['javascript:alert(1)', 'data:text/html,unsafe', 'blob:https://example.com/id'])('rejects unsafe navigation scheme %s', (href) => {
        const windowInstance = new Window({ url: 'http://localhost/' });
        const anchor = windowInstance.document.createElement('a');
        anchor.setAttribute('href', href);
        /** @type {string[]} */
        const navigations = [];
        expect(resumeCapturedNativeAction(anchor, 'click', { navigate: (url) => navigations.push(url) })).toBe(false);
        expect(navigations).toEqual([]);
    });
});
