// @ts-check
/**
 * Deferred, replay-safe event-delegation hydration.
 *
 * Two ideas, unified:
 *
 *  1. Event replay (Angular-style). A tiny inline script (EVENT_CAPTURE_SCRIPT)
 *     runs in the document <head> before the runtime loads. It records click,
 *     submit, keyboard, and compiled custom-handler events (plus internal-anchor
 *     clicks) into a queue and neutralizes native actions where necessary, so
 *     nothing a user does during the pre-hydration "dead zone" is lost.
 *
 *  2. Lazy / intent delegation (Astro-style). The runtime does not register its
 *     delegated `document` listeners synchronously during hydration. It waits for
 *     the browser to go idle (requestIdleCallback) — or flushes immediately the
 *     moment the user shows intent (a captured interaction) — keeping event
 *     wiring off the critical path. On flush it registers the compile-time event
 *     set and replays the captured queue through the real delegation.
 */
export { EVENT_CAPTURE_SCRIPT } from './event-capture-script.js';

/**
 * Restores a native action that the dead-zone capture deliberately prevented
 * when its island could not be activated. Returns whether a fallback existed.
 *
 * @param {Element} target
 * @param {string} type
 * @param {{ navigate?: (href: string) => void, submit?: (form: HTMLFormElement) => void }} [actions]
 */
export function resumeCapturedNativeAction(target, type, actions = {}) {
    if (type === 'click') {
        /** @type {Element | null} */
        let anchor = target;
        while (anchor && !(anchor.tagName === 'A' && anchor.hasAttribute('href')))
            anchor = anchor.parentElement;
        const href = anchor?.getAttribute('href');
        if (href) {
            let url;
            try {
                url = new URL(href, globalThis.location?.href ?? 'http://localhost/');
            }
            catch {
                return false;
            }
            if (!['http:', 'https:', 'file:', 'mailto:', 'tel:'].includes(url.protocol))
                return false;
            (actions.navigate ?? ((url) => location.assign(url)))(href);
            return true;
        }
    }
    if (type === 'submit') {
        /** @type {Element | null} */
        let candidate = target;
        while (candidate && candidate.tagName !== 'FORM')
            candidate = candidate.parentElement;
        const form = /** @type {HTMLFormElement | null} */ (candidate);
        if (form) {
            (actions.submit ?? ((candidate) => candidate.submit()))(form);
            return true;
        }
    }
    return false;
}

/**
 * @typedef {{ type: string, target: EventTarget & { isConnected?: boolean }, kind?: string, init?: EventInit & Record<string, unknown> }} CapturedEvent
 * @typedef {{ queue: CapturedEvent[], onIntent: ((record: CapturedEvent) => void) | null, stop: () => void }} EventCapture
 */

/**
 * Reconstructs the browser event captured before the runtime loaded.
 * @param {EventTarget} target
 * @param {CapturedEvent} record
 * @returns {Event}
 */
export function createCapturedReplayEvent(target, record) {
    const view = /** @type {any} */ (/** @type {any} */ (target).ownerDocument?.defaultView ?? globalThis);
    const Constructor = record.kind && typeof view[record.kind] === 'function'
        ? view[record.kind]
        : view.Event;
    try {
        return new Constructor(record.type, record.init ?? { bubbles: true, composed: true, cancelable: true });
    }
    catch {
        return new view.Event(record.type, { bubbles: true, composed: true, cancelable: true });
    }
}

/**
 * @param {EventCapture | null | undefined} capture
 * @param {(target: any, type: string, record?: CapturedEvent) => void} dispatch
 * @param {((target: any, type: string) => void | Promise<void>) | undefined} beforeReplay
 * @param {((error: unknown, record: CapturedEvent) => void | Promise<void>) | undefined} onReplayError
 */
async function replayCaptured(capture, dispatch, beforeReplay, onReplayError) {
    if (!capture)
        return;
    if (typeof capture.stop === 'function')
        capture.stop();
    const queue = Array.isArray(capture.queue) ? capture.queue.splice(0) : [];
    for (const record of queue) {
        const target = record.target;
        // Re-dispatch a fresh event so the now-registered delegation handles it
        // (SPA nav for anchors, rerender for handlers). Skip targets removed by
        // the hydration morph.
        if (target && (target.isConnected ?? true)) {
            try {
                if (beforeReplay)
                    await beforeReplay(target, record.type);
                dispatch(target, record.type, record);
            }
            catch (error) {
                if (onReplayError)
                    await onReplayError(error, record);
                else
                    console.error('[tachyon] Failed to prepare captured event for replay:', error);
            }
        }
    }
}

/**
 * Builds the deferred-delegation controller. Dependencies are injected so the
 * scheduling/replay logic is testable without a real browser.
 * @param {{
 *   ensure: (eventName: string) => void,
 *   requestIdle?: (cb: () => void) => void,
 *   getCapture?: () => EventCapture | null | undefined,
 *   dispatch?: (target: any, type: string, record?: CapturedEvent) => void,
 *   beforeReplay?: (target: any, type: string) => void | Promise<void>,
 *   onReplayError?: (error: unknown, record: CapturedEvent) => void | Promise<void>,
 * }} options
 */
export function createDeferredDelegation(options) {
    const ensure = options.ensure;
    const requestIdle = options.requestIdle
        ?? ((cb) => {
            const ric = /** @type {any} */ (globalThis).requestIdleCallback;
            if (typeof ric === 'function')
                ric(cb);
            else
                setTimeout(cb, 1);
        });
    const getCapture = options.getCapture
        ?? (() => /** @type {any} */ (globalThis).__tacEventCapture);
    const dispatch = options.dispatch
        ?? ((target, _type, record) => target.dispatchEvent(
            record
                ? createCapturedReplayEvent(target, record)
                : new Event(_type, { bubbles: true, composed: true, cancelable: true })
        ));

    /** @type {Set<string>} */
    const pending = new Set();
    let scheduled = false;
    let flushed = false;
    /** @type {Promise<void> | null} */
    let flushing = null;

    function flush() {
        if (flushing)
            return flushing;
        if (flushed)
            return Promise.resolve();
        if (!options.beforeReplay) {
            for (const eventName of pending)
                ensure(eventName);
            pending.clear();
            flushed = true;
            void replayCaptured(getCapture(), dispatch, undefined, options.onReplayError);
            return Promise.resolve();
        }
        flushing = (async () => {
            for (const eventName of pending)
                ensure(eventName);
            pending.clear();
            flushed = true;
            await replayCaptured(getCapture(), dispatch, options.beforeReplay, options.onReplayError);
        })().finally(() => { flushing = null; });
        return flushing;
    }

    /** @param {string[]} [eventNames] */
    function schedule(eventNames) {
        if (eventNames)
            for (const eventName of eventNames)
                pending.add(eventName);
        if (flushed) {
            void (async () => {
                for (const eventName of pending)
                    ensure(eventName);
                pending.clear();
            })(); // late component after hydration — register immediately
            return;
        }
        if (scheduled)
            return;
        scheduled = true;
        const capture = getCapture();
        if (capture)
            capture.onIntent = flush; // flush the instant the user interacts
        if (capture && Array.isArray(capture.queue) && capture.queue.length > 0) {
            void flush(); // interaction already happened before we scheduled
            return;
        }
        requestIdle(() => { void flush(); }); // otherwise hydrate delegation when the browser is idle
    }

    return { schedule, flush };
}
