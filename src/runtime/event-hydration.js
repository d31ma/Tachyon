// @ts-check
/**
 * Deferred, replay-safe event-delegation hydration.
 *
 * Two ideas, unified:
 *
 *  1. Event replay (Angular-style). A tiny inline script (EVENT_CAPTURE_SCRIPT)
 *     runs in the document <head> before the runtime loads. It records click /
 *     submit interactions that land on a Tac handler (a `data-tac-on-*` marker or
 *     an internal anchor) into a queue and neutralizes their default, so nothing
 *     a user does during the pre-hydration "dead zone" is lost.
 *
 *  2. Lazy / intent delegation (Astro-style). The runtime does not register its
 *     delegated `document` listeners synchronously during hydration. It waits for
 *     the browser to go idle (requestIdleCallback) — or flushes immediately the
 *     moment the user shows intent (a captured interaction) — keeping event
 *     wiring off the critical path. On flush it registers the compile-time event
 *     set and replays the captured queue through the real delegation.
 */

/**
 * Inline, dependency-free capture script for the SSR shell. Must run before the
 * runtime module. Sets `window.__tacEventCapture = { queue, onIntent, stop }`.
 */
export const EVENT_CAPTURE_SCRIPT = `(function(){
  var TYPES=['click','submit'];
  var q=[];
  function tacTarget(el){
    for(var n=el; n && n!==document.body && n!==document; n=n.parentElement){
      if(n.tagName==='A' && n.hasAttribute('href')) return n;
      var a=n.attributes; for(var i=0;i<a.length;i++){ if(a[i].name.indexOf('data-tac-on-')===0) return n; }
    }
    return null;
  }
  function rec(e){
    var t=e.target; if(!t || t.nodeType!==1) return;
    var handler=tacTarget(t); if(!handler) return;
    q.push({type:e.type, target:handler});
    if(e.type==='click'||e.type==='submit') e.preventDefault();
    var c=window.__tacEventCapture; if(c && c.onIntent) c.onIntent();
  }
  for(var i=0;i<TYPES.length;i++) document.addEventListener(TYPES[i], rec, true);
  window.__tacEventCapture={queue:q, onIntent:null, stop:function(){ for(var i=0;i<TYPES.length;i++) document.removeEventListener(TYPES[i], rec, true); }};
})();`;

/**
 * @typedef {{ type: string, target: EventTarget & { isConnected?: boolean } }} CapturedEvent
 * @typedef {{ queue: CapturedEvent[], onIntent: (() => void) | null, stop: () => void }} EventCapture
 */

/**
 * @param {EventCapture | null | undefined} capture
 * @param {(target: any, type: string) => void} dispatch
 */
function replayCaptured(capture, dispatch) {
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
        if (target && (target.isConnected ?? true))
            dispatch(target, record.type);
    }
}

/**
 * Builds the deferred-delegation controller. Dependencies are injected so the
 * scheduling/replay logic is testable without a real browser.
 * @param {{
 *   ensure: (eventName: string) => void,
 *   requestIdle?: (cb: () => void) => void,
 *   getCapture?: () => EventCapture | null | undefined,
 *   dispatch?: (target: any, type: string) => void,
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
        ?? ((target, type) => target.dispatchEvent(new Event(type, { bubbles: true, composed: true, cancelable: true })));

    /** @type {Set<string>} */
    const pending = new Set();
    let scheduled = false;
    let flushed = false;

    function flush() {
        for (const eventName of pending)
            ensure(eventName);
        pending.clear();
        if (flushed)
            return; // listeners were already live; we only needed to register the new types
        flushed = true;
        replayCaptured(getCapture(), dispatch);
    }

    /** @param {string[]} [eventNames] */
    function schedule(eventNames) {
        if (eventNames)
            for (const eventName of eventNames)
                pending.add(eventName);
        if (flushed) {
            flush(); // late component after hydration — register immediately
            return;
        }
        if (scheduled)
            return;
        scheduled = true;
        const capture = getCapture();
        if (capture)
            capture.onIntent = flush; // flush the instant the user interacts
        if (capture && Array.isArray(capture.queue) && capture.queue.length > 0) {
            flush(); // interaction already happened before we scheduled
            return;
        }
        requestIdle(flush); // otherwise hydrate delegation when the browser is idle
    }

    return { schedule, flush };
}
