// @ts-check
import { cleanBooleanAttrs, createFocusLease, createValueEventDetail, findEventTarget, findNavigationTarget, morphChildren, parseFragment, parseParams, repointCurrentTarget, resolveHandler } from './dom-helpers.js';
import { cacheModuleResponse, clearStaticCache, precacheModules } from './static-cache.js';
import { createDeferredDelegation, resumeCapturedNativeAction } from './event-hydration.js';
import { IslandRuntime } from './island-runtime.js';
import { createComponentRegistry } from './component-registry.js';
import { ServiceWorkerPolicy } from './service-worker-policy.js';

/** @type {string} */
const ASSET_PREFIX = '__TACHYON_ASSET_PREFIX__';

/**
 * @typedef {{ path: string, allowSelf: boolean }} WrapperPageEntry
 * @typedef {Record<string, WrapperPageEntry>} WrapperPageManifest
 * @typedef {Record<string, Record<string, string>>} RouteManifest
 * @typedef {(elemId?: string | null, event?: unknown, compId?: string | null) => Promise<string>} TacRender
 * @typedef {(props?: unknown) => Promise<TacRender> | TacRender} TacFactory
 * Rooted terminology: `platform` is the form factor (desktop | mobile | web);
 * `environment` and `os` are synonyms for the concrete host
 * (windows | macos | linux | android | ios | web); `target` is the bundle output.
 * @typedef {'desktop' | 'mobile' | 'web'} TacPlatform
 * @typedef {'macos' | 'windows' | 'linux' | 'android' | 'ios' | 'web'} TacOperatingSystem
 * @typedef {'macos' | 'windows' | 'linux' | 'android' | 'ios' | 'unknown'} TacDetectedOS
 * @typedef {{
 *   target: 'web' | 'macos' | 'windows' | 'linux' | 'android' | 'ios',
 *   platform: TacPlatform,
 *   environment: TacOperatingSystem,
 *   os: TacOperatingSystem,
 *   browserOS: TacDetectedOS,
 *   native: boolean,
 *   web: boolean,
 *   desktop: boolean,
 *   mobile: boolean
 * }} TacPlatformContext
 * @typedef {{
 *   version: string,
 *   modules: Map<string, TacFactory>,
 *   platform: TacPlatformContext,
 *   register: (path: string, factory: TacFactory) => TacFactory,
 *   load: (path: string) => Promise<TacFactory>,
 *   delegateEvents?: (eventNames: string[]) => void,
 *   registerComponentRender?: (
 *     hostId: string,
 *     render: (elemId?: string | null, event?: unknown, componentId?: string | null) => Promise<string>,
 *     componentId: string
 *   ) => void
 * }} TacGlobal
 */

/** @returns {TacDetectedOS} */
function detectBrowserOS() {
    const nav = /** @type {(Navigator & { userAgentData?: { platform?: string } }) | null} */ (typeof navigator !== 'undefined' ? navigator : null);
    const ua = String(nav?.userAgent || '').toLowerCase();
    const navPlatform = String(nav?.userAgentData?.platform || nav?.platform || '').toLowerCase();
    if (ua.includes('android')) return 'android';
    if (ua.includes('iphone') || ua.includes('ipad') || ua.includes('ios')) return 'ios';
    if (navPlatform.includes('mac') || ua.includes('mac os')) return 'macos';
    if (navPlatform.includes('win') || ua.includes('windows')) return 'windows';
    if (navPlatform.includes('linux') || ua.includes('linux')) return 'linux';
    return 'unknown';
}

/** @param {string} name @param {string} fallback */
function readMetaContent(name, fallback) {
    const node = typeof document !== 'undefined'
        ? document.querySelector(`meta[name="${name}"]`)
        : null;
    return node && 'content' in node && typeof node.content === 'string' && node.content
        ? node.content
        : fallback;
}

/** @param {string} target @returns {TacPlatform} */
function platformForTarget(target) {
    if (target === 'macos' || target === 'windows' || target === 'linux') return 'desktop';
    if (target === 'ios' || target === 'android') return 'mobile';
    return 'web';
}

/** @returns {TacPlatformContext} */
function resolvePlatformContext() {
    const browserOS = detectBrowserOS();
    const target = /** @type {TacPlatformContext['target']} */ (readMetaContent('tachyon-target', 'web'));
    const platform = /** @type {TacPlatform} */ (readMetaContent('tachyon-platform', platformForTarget(target)));
    // `environment` and `os` are synonyms: the OS for native bundles, 'web'
    // for browser bundles. The detected visitor OS stays on `browserOS`.
    const os = /** @type {TacOperatingSystem} */ (readMetaContent('tachyon-os', target === 'web' ? 'web' : target));
    return Object.freeze({
        target,
        platform,
        environment: os,
        os,
        browserOS,
        native: target !== 'web',
        web: platform === 'web',
        desktop: platform === 'desktop',
        mobile: platform === 'mobile',
    });
}

/** @returns {TacGlobal} */
function getTacGlobal() {
    const existing = /** @type {TacGlobal | undefined} */ (window.Tac);
    const platformContext = resolvePlatformContext();
    if (existing?.modules
        && typeof existing.register === 'function'
        && typeof existing.load === 'function') {
        existing.platform = platformContext;
        Object.assign(globalThis, {
            platform: platformContext.platform,
            environment: platformContext.environment,
            os: platformContext.os,
            target: platformContext.target,
        });
        return /** @type {TacGlobal} */ (existing);
    }

    /** @type {Map<string, TacFactory>} */
    const modules = existing?.modules ?? new Map();
    /** @type {TacGlobal} */
    const tac = {
        version: '1',
        modules,
        platform: platformContext,
        register(modulePath, factory) {
            modules.set(modulePath, factory);
            return factory;
        },
        async load(modulePath) {
            const registered = modules.get(modulePath);
            if (registered)
                return registered;
            const module = await import(modulePath);
            // Cache the module response via Cache API for instant loads on
            // repeat visits. Fire-and-forget — don't block rendering on it.
            fetch(modulePath).then((res) => cacheModuleResponse(modulePath, res)).catch(() => {});
            if (typeof module.default === 'function')
                return /** @type {TacFactory} */ (module.default);
            const loaded = modules.get(modulePath);
            if (loaded)
                return loaded;
            throw new Error(`Tac module "${modulePath}" did not export or register a renderer`);
        },
    };

    const registry = /** @type {TacGlobal} */ (Object.assign(existing ?? {}, tac));
    window.Tac = /** @type {any} */ (registry);
    Object.assign(globalThis, {
        platform: registry.platform.platform,
        environment: registry.platform.environment,
        os: registry.platform.os,
        target: registry.platform.target,
    });
    return registry;
}

/** @type {TacRender | null} */
let pageRender = null;
/** @type {TacRender | null} */
let wrapperRender = null;
/** @type {string | null} */
let currentWrapperPath = null;
/** @type {string | null} */
let currentPageURL = null;
let previousHTML = '';
let freshNavigation = false;

/** @type {Map<string, Record<string, string>>} */
const routes = new Map();
/** @type {WrapperPageManifest} */
const wrapperPages = {};
const routeManifestJSON = '{"__tachyonPlaceholder":true}';
const wrapperManifestJSON = '{"__tachyonShellPlaceholder":true}';
const componentManifestJSON = '{"__tachyonComponentPlaceholder":true}';
/** @type {Record<string, string>} */
const componentModules = JSON.parse(componentManifestJSON);
/** @type {Record<string, string>} */
const slugs = {};
/** @type {unknown[]} */
let params = [];
const tac = getTacGlobal();
const delegatedEvents = new Set();
/** @type {IslandRuntime | null} */
let islandRuntime = null;

/**
 * Static HTML is rendered before the browser can read sessionStorage /
 * localStorage. Patch marked persistent text nodes immediately so `$`
 * (sessionStorage) and `$$` (localStorage) fields do not visibly reset
 * while the full Tac route hydration is still loading manifests/modules.
 */
function prehydratePersistentText() {
    try {
        const nodes = document.querySelectorAll('[data-tac-persist-field]');
        for (const node of nodes) {
            try {
                if (!(node instanceof HTMLElement))
                    continue;
                const fieldName = node.dataset.tacPersistField;
                const scope = node.closest('[data-tac-module]');
                if (!(scope instanceof HTMLElement) || !fieldName)
                    continue;
                const modulePath = scope.dataset.tacModule;
                if (!modulePath || !scope.id)
                    continue;
                const storageKey = `tac:${modulePath}:${scope.id}:${fieldName}`;
                const storage = fieldName.startsWith('$$') ? localStorage : sessionStorage;
                const stored = storage.getItem(storageKey);
                if (stored === null)
                    continue;
                const value = JSON.parse(stored);
                node.textContent = value === null || value === undefined ? '' : String(value);
            }
            catch {
                continue;
            }
        }
    }
    catch {
        return;
    }
}

prehydratePersistentText();

async function loadManifests() {
    const routeData = /** @type {RouteManifest} */ (JSON.parse(routeManifestJSON));
    const wrapperData = /** @type {WrapperPageManifest} */ (JSON.parse(wrapperManifestJSON));

    routes.clear();
    for (const [routePath, routeConfig] of Object.entries(routeData)) {
        routes.set(routePath, routeConfig);
    }

    for (const key of Object.keys(wrapperPages))
        delete wrapperPages[key];
    Object.assign(wrapperPages, wrapperData);
}

Promise.all([loadManifests()]).then(() => {
    if (document.querySelector('[data-tac-island], [data-tac-island-static]'))
        void adoptCurrentDocument();
    else
        navigate(currentNavigationTarget());
});

/** @returns {string} */
function currentNavigationTarget() {
    return normalizeNavigationTarget(location.pathname + location.search + location.hash);
}

/** @param {string} target */
function normalizeNavigationTarget(target) {
    const fullTarget = String(target || '/');
    const suffixStart = fullTarget.search(/[?#]/);
    const pathname = suffixStart >= 0 ? fullTarget.slice(0, suffixStart) : fullTarget;
    const suffix = suffixStart >= 0 ? fullTarget.slice(suffixStart) : '';
    if (location.protocol === 'file:' && (!pathname || pathname.endsWith('/index.html')))
        return `/${suffix}`;
    return fullTarget;
}

/** @param {string} eventName */
function ensureDelegatedEvent(eventName) {
    if (!eventName || delegatedEvents.has(eventName))
        return;
    delegatedEvents.add(eventName);
    document.addEventListener(eventName, (event) => handleDelegatedEvent(eventName, event));
}

// Deferred, replay-safe delegation. Registration of the (compile-time) event set
// is held off the critical path until the browser is idle, or until the user
// interacts — whichever first — at which point any interactions captured during
// the pre-hydration dead zone are replayed. See event-hydration.js.
const deferredDelegation = createDeferredDelegation({
    ensure: ensureDelegatedEvent,
    beforeReplay: async (target) => islandRuntime?.promote(target),
    onReplayError: (error, record) => {
        console.error('[tachyon] Island intent activation failed:', error);
        resumeCapturedNativeAction(/** @type {Element} */ (record.target), record.type);
    },
});

// Compile-time event registration: each compiled module calls this once at setup
// with the event types its template uses (collected by the compiler). Exposed on
// the Tac global so component modules, loaded separately, can reach the renderer.
/** @param {string[]} eventNames */
tac.delegateEvents = (eventNames) => {
    if (Array.isArray(eventNames))
        deferredDelegation.schedule(eventNames);
};

// Component-scoped re-render registry: every rendered component registers its
// host id → its render closure (via the Tac global), so an interaction or state
// change inside a component re-renders just that subtree instead of the whole
// page. Generalizes the `lazy` boundary (lazyRenders) to all components.
const componentRegistry = createComponentRegistry();
tac.registerComponentRender = componentRegistry.register;
islandRuntime = new IslandRuntime({
    root: document,
    importer: async (modulePath) => tac.load(modulePath),
    componentModules,
    registerComponentRender: componentRegistry.register,
    onIntentError: (_error, target, type) => resumeCapturedNativeAction(target, type),
});

/** @param {Element} element */
function preserveRuntimeBoundary(element) {
    return lazyRenders.has(element.id)
        || element.hasAttribute('data-tac-island')
        || element.hasAttribute('data-tac-island-static');
}

/**
 * Locates the closest ancestor of the event target that declares a handler for
 * `eventName`, using the event's composed path (single native call, and correct
 * for composed events crossing shadow boundaries) with a DOM-walk fallback.
 * @param {Event} event
 * @param {string} eventName
 * @returns {Element | null}
 */
function findDelegatedTarget(event, eventName) {
    const marker = `data-tac-on-${eventName.replaceAll(':', '__')}`;
    const path = typeof event.composedPath === 'function' ? event.composedPath() : null;
    if (path) {
        for (const node of path) {
            if (node === document)
                break;
            if (node instanceof Element && node.hasAttribute(marker))
                return node;
        }
        return null;
    }
    return findEventTarget(event.target instanceof Element ? event.target : null, eventName);
}

/**
 * @param {string} eventName
 * @param {Event} event
 */
function handleDelegatedEvent(eventName, event) {
    if (eventName === 'click') {
        const navigationTarget = findNavigationTarget(event);
        if (navigationTarget) {
            const href = navigationTarget.getAttribute('href');
            if (!href)
                return;
            const url = new URL(href, location.origin);
            const hasModifierKey = event instanceof MouseEvent
                && (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey);
            const isPrimaryClick = !(event instanceof MouseEvent) || event.button === 0;
            const target = navigationTarget.getAttribute('target');
            const isSameDocumentHash = url.origin === location.origin
                && url.pathname === location.pathname
                && url.search === location.search
                && Boolean(url.hash);
            const allowSpaNavigation = url.origin === location.origin
                && !navigationTarget.hasAttribute('download')
                && !hasModifierKey
                && isPrimaryClick
                && (!target || target === '_self')
                && canHandleClientNavigation(url);
            if (isSameDocumentHash && allowSpaNavigation) {
                event.preventDefault();
                navigateToHash(url);
                return;
            }
            if (allowSpaNavigation) {
                event.preventDefault();
                navigate(url.pathname + url.search + url.hash);
                return;
            }
        }
    }

    const target = findDelegatedTarget(event, eventName);
    if (!target)
        return;

    if (eventName === 'click' || eventName === 'submit')
        event.preventDefault();
    dispatchAction(target, event);
}

/** @param {string} hash */
function scrollToHash(hash) {
    if (!hash) {
        window.scrollTo({ top: 0, left: 0 });
        return;
    }
    let id = hash.slice(1);
    try {
        id = decodeURIComponent(id);
    }
    catch {
        // Invalid percent escapes cannot identify a DOM target.
    }
    document.getElementById(id)?.scrollIntoView({ block: 'start' });
}

/** @param {URL} url */
function navigateToHash(url) {
    const target = url.pathname + url.search + url.hash;
    if (location.pathname + location.search + location.hash === target)
        history.replaceState({}, '', target);
    else
        history.pushState({}, '', target);
    requestAnimationFrame(() => scrollToHash(url.hash));
}

// `click` always participates (SPA navigation on internal anchors), scheduled
// through the same deferred/replay path so dead-zone clicks aren't lost.
deferredDelegation.schedule(['click']);

// The id the compiler keys handler/binding dispatch off. It's normally the
// element's generated `tc-<hash>` id, but when the author supplies their own
// `id` the compiler stashes the dispatch id in `data-tac-id` so the author id
// can't clobber it (#110).
/** @param {Element} element @returns {string} */
function dispatchId(element) {
    return element.getAttribute('data-tac-id') || element.id;
}

for (const eventName of ['input', 'change', 'sl-input', 'sl-change']) {
    document.addEventListener(eventName, (event) => {
        const element = event.target instanceof Element ? event.target : null;
        if (!element || !element.hasAttribute('value'))
            return;
        const id = dispatchId(element);
        if (!id)
            return;
        const eventDetail = createValueEventDetail(/** @type {Element & { value?: unknown }} */ (element), event);
        void enqueueRenderPass(() => rerender(id, eventDetail));
    });
}

window.addEventListener('popstate', () => navigate(currentNavigationTarget()));

/**
 * @param {Element} element
 * @param {unknown} eventDetail
 */
function dispatchAction(element, eventDetail) {
    // Delegation listens on `document` and dispatch is deferred to a microtask,
    // so by the time the handler runs the browser has reset the event's
    // `currentTarget`. Re-point it to the matched (marker-bearing) element so
    // handlers see native addEventListener semantics (#110 comment).
    repointCurrentTarget(eventDetail, element);
    void enqueueRenderPass(() => rerender(dispatchId(element), eventDetail));
}

/** @param {string} elementId */
function findLazyAncestor(elementId) {
    /** @type {HTMLElement | null} */
    let element = document.getElementById(elementId);
    while (element) {
        if (lazyRenders.has(element.id))
            return element;
        element = element.parentElement;
    }
    return null;
}

/**
 * Re-renders a single component's subtree into its host element. With a
 * triggerId it first runs the matching handler (event path); without one it
 * just re-renders with current state (reactive path).
 * @param {HTMLElement} host
 * @param {{ render: TacRender, compId: string }} entry
 * @param {string | null} triggerId
 * @param {unknown} [eventDetail]
 */
async function rerenderComponent(host, entry, triggerId, eventDetail) {
    if (triggerId !== null)
        await runHandlerPass(entry.render(triggerId, eventDetail, entry.compId));
    const html = await entry.render(null, undefined, entry.compId);
    morphChildren(host, parseFragment(html), {
        preserveElement: preserveRuntimeBoundary,
    });
    postPatch();
}

/**
 * Render passes share module-level state (render contexts, id generation, the
 * component registry), so two interleaved passes can transiently blank a
 * subtree — e.g. a signal fanning out to several components at once. Every
 * external trigger funnels through one chain so passes run strictly in order.
 * @type {Promise<void>}
 */
let renderChain = Promise.resolve();
/** @param {() => Promise<void> | void} pass */
function enqueueRenderPass(pass) {
    const next = renderChain.then(pass, pass);
    renderChain = next.then(() => undefined, () => undefined);
    return next;
}

/**
 * Runs the trigger pass of a render whose sole purpose is to invoke the matching
 * event handler; its HTML output is discarded (the following state-only render
 * produces the DOM). A handler can mutate state mid-render so that a later
 * interpolation in the same pass dereferences the just-cleared value (e.g. a
 * handler on a `<logic :if="menu">` child sets `menu = null`, then `{menu.x}`
 * later in the block throws). Swallow that spurious render error so the clean
 * state-only render still runs — the handler already applied its state change
 * (#112). A genuine handler error is surfaced but must not strand the DOM.
 * @param {Promise<unknown>} triggerRender
 */
async function runHandlerPass(triggerRender) {
    try {
        await triggerRender;
    }
    catch (error) {
        console.error('[tachyon] handler render pass error (continuing with state-only render):', error);
    }
}

/**
 * @param {string} triggerId
 * @param {unknown} [eventDetail]
 */
async function rerender(triggerId, eventDetail) {
    const focusLease = createFocusLease(triggerId);
    try {
        await rerenderView(triggerId, eventDetail);
    }
    finally {
        focusLease.restore();
    }
}

/**
 * @param {string} triggerId
 * @param {unknown} [eventDetail]
 */
async function rerenderView(triggerId, eventDetail) {
    const lazyContainer = findLazyAncestor(triggerId);
    if (lazyContainer) {
        const render = lazyRenders.get(lazyContainer.id);
        if (!render)
            return;
        await runHandlerPass(render(triggerId, eventDetail, lazyContainer.id));
        const html = await render(null, undefined, lazyContainer.id);
        morphChildren(lazyContainer, parseFragment(html), {
            preserveElement: preserveRuntimeBoundary,
        });
        postPatch();
        return;
    }

    // Scope the update to the nearest component owning the trigger, instead of
    // re-rendering the whole page. Falls through to the page path when the
    // trigger isn't inside a (scopable) component.
    const component = componentRegistry.findAncestor(triggerId);
    if (component) {
        await rerenderComponent(component.host, component.entry, triggerId, eventDetail);
        return;
    }

    const inSlot = wrapperRender ? isInsideSlot(triggerId) : false;
    if (wrapperRender && inSlot) {
        if (!pageRender)
            return;
        await runHandlerPass(pageRender(triggerId, eventDetail));
        patchSlot(await pageRender());
        return;
    }

    if (wrapperRender) {
        await rerenderWrapper(triggerId, eventDetail);
        return;
    }

    if (!pageRender)
        return;
    await runHandlerPass(pageRender(triggerId, eventDetail));
    patchBody(await pageRender());
}

async function refreshCurrentView() {
    if (!pageRender)
        return;
    previousHTML = '';

    if (wrapperRender) {
        const [wrapperHTML, pageHTML] = await Promise.all([
            wrapperRender(),
            pageRender(),
        ]);
        const tempDoc = new DOMParser().parseFromString(`<body>${wrapperHTML}</body>`, 'text/html');
        const slot = tempDoc.getElementById('tc-page-slot');
        if (slot)
            slot.innerHTML = pageHTML;
        morphChildren(document.body, parseFragment(tempDoc.body.innerHTML), {
            preserveElement: preserveRuntimeBoundary,
        });
        previousHTML = pageHTML;
    }
    else {
        patchBody(await pageRender());
    }

    postPatch();
}

/** @param {string} elementId */
function isInsideSlot(elementId) {
    let node = document.getElementById(elementId);
    while (node) {
        if (node.id === 'tc-page-slot')
            return true;
        node = node.parentElement;
    }
    return false;
}

/**
 * @param {string} [triggerId]
 * @param {unknown} [eventDetail]
 */
async function rerenderWrapper(triggerId, eventDetail) {
    if (!wrapperRender)
        return;
    const slotHTML = document.getElementById('tc-page-slot')?.innerHTML ?? '';
    if (triggerId)
        await runHandlerPass(wrapperRender(triggerId, eventDetail));
    const html = await wrapperRender();
    document.body.innerHTML = html;
    const slot = document.getElementById('tc-page-slot');
    if (slot)
        slot.innerHTML = slotHTML;
    postPatch();
}

/** @param {string} pathname */
function resolveWrapperPage(pathname) {
    if (pathname !== '/') {
        const segments = pathname.split('/');
        for (let segmentCount = segments.length; segmentCount >= 1; segmentCount -= 1) {
            const prefix = segments.slice(0, segmentCount).join('/') || '/';
            const entry = wrapperPages[prefix];
            if (entry && (entry.allowSelf || prefix !== pathname))
                return entry.path;
        }
    }

    const rootEntry = wrapperPages['/'];
    if (rootEntry && (rootEntry.allowSelf || pathname !== '/'))
        return rootEntry.path;
    return null;
}

/** @param {string} html */
function patchSlot(html) {
    const slot = document.getElementById('tc-page-slot');
    if (!slot || (!html && !previousHTML) || html === previousHTML)
        return;
    previousHTML = html;
    if (freshNavigation) {
        slot.innerHTML = html;
    }
    else {
        morphChildren(slot, parseFragment(html), {
            preserveElement: preserveRuntimeBoundary,
        });
    }
    postPatch();
}

/** @param {string} html */
function patchBody(html) {
    if (!html || html === previousHTML)
        return;
    previousHTML = html;
    if (freshNavigation) {
        document.body.innerHTML = html;
    }
    else {
        morphChildren(document.body, parseFragment(html), {
            preserveElement: preserveRuntimeBoundary,
        });
    }
    postPatch();
}

const lazyLoaded = new Set();
/** @type {Map<string, TacRender>} */
const lazyRenders = new Map();
const lazyObserver = new IntersectionObserver((entries) => {
    for (const entry of entries) {
        if (!entry.isIntersecting)
            continue;
        const element = entry.target instanceof HTMLElement ? entry.target : null;
        if (!element)
            continue;
        const id = element.id;
        if (lazyLoaded.has(id))
            continue;
        lazyLoaded.add(id);
        lazyObserver.unobserve(element);
        loadLazyComponent(element);
    }
}, { rootMargin: '100px' });

/** @param {HTMLElement} element */
async function loadLazyComponent(element) {
    const componentName = element.dataset.lazyComponent;
    const modulePath = element.dataset.lazyPath;
    if (!modulePath)
        return;
    const propsRaw = element.dataset.lazyProps || '';
    const props = propsRaw ? JSON.parse(decodeURIComponent(propsRaw)) : null;

    try {
        const factory = await tac.load(modulePath);
        const render = await factory(props);
        lazyRenders.set(element.id, render);
        element.innerHTML = await render(null, undefined, element.id);
        element.removeAttribute('data-lazy-component');
        element.removeAttribute('data-lazy-path');
        element.removeAttribute('data-lazy-props');
        postPatch();
    }
    catch (error) {
        console.error(`[tachyon] Failed to load lazy component "${componentName}":`, error);
    }
}

function observeLazyComponents() {
    const placeholders = document.querySelectorAll('[data-lazy-component]');
    for (const element of placeholders) {
        if (element instanceof HTMLElement && !lazyLoaded.has(element.id))
            lazyObserver.observe(element);
    }
}

function postPatch() {
    cleanBooleanAttrs();
    observeLazyComponents();
    islandRuntime?.scan(document);

    const queue = /** @type {Array<() => void | Promise<void>> | undefined} */ (window.__tc_onMount_queue__);
    // Controllers that register onMount after this flush (e.g. async-loaded
    // Dart companions) run immediately instead of waiting for a next patch.
    window.__tc_onMount_flushed__ = true;
    if (queue?.length) {
        window.__tc_onMount_queue__ = [];
        for (const fn of queue) {
            try {
                const result = fn();
                if (result instanceof Promise)
                    result.catch((error) => console.error('[tachyon] onMount callback error:', error));
            }
            catch (error) {
                console.error('[tachyon] onMount callback error:', error);
            }
        }
    }

    if (freshNavigation) {
        window.dispatchEvent(new CustomEvent('tachyon:navigate', { detail: { pathname: location.pathname } }));
        requestAnimationFrame(() => scrollToHash(location.hash));
    }
    freshNavigation = false;
}

/**
 * Initializes render closures and event manifests against the already
 * prerendered document. Island boundaries keep their original nodes; no
 * initial body patch occurs.
 */
async function adoptCurrentDocument() {
    const pathname = location.pathname || '/';
    let handler = '/';
    try {
        handler = resolvePageHandler(pathname);
    }
    catch {
        handler = '';
    }
    const handlerPath = handler.replaceAll('/:', '/_');
    const pageURL = handler === '/'
        ? `${ASSET_PREFIX}pages/tac.js`
        : handler
            ? `${ASSET_PREFIX}pages${handlerPath}/tac.js`
            : `${ASSET_PREFIX}pages/404.js`;
    const wrapperPath = resolveWrapperPage(pathname);
    try {
        const [pageFactory, wrapperFactory] = await Promise.all([
            tac.load(pageURL),
            wrapperPath ? tac.load(wrapperPath) : Promise.resolve(null),
        ]);
        pageRender = await pageFactory();
        wrapperRender = wrapperFactory ? await wrapperFactory() : null;
        currentPageURL = pageURL;
        currentWrapperPath = wrapperPath;
        // Execute once to register eager child render closures and compile-time
        // event sets. The generated HTML is intentionally discarded.
        if (wrapperRender)
            await wrapperRender();
        await pageRender();
        islandRuntime?.scan(document);
        postPatch();
    }
    catch (error) {
        console.error('[tachyon] Failed to adopt prerendered Tac document:', error);
        // Preserve useful SSR HTML on failure rather than blanking the page.
        islandRuntime?.scan(document);
    }
}

/** @param {string} html */
function containsIslandBoundary(html) {
    return html.includes('data-tac-island') || html.includes('data-tac-island-static');
}

/** @param {string} url */
function navigateToIslandDocument(url) {
    // Island routes require their prerendered HTML. A document navigation keeps
    // strict `never` islands module-free and gives deferred islands real SSR DOM
    // to adopt instead of client-created empty boundaries.
    location.assign(url);
}

/** @param {string} target */
function navigate(target) {
    const fullTarget = normalizeNavigationTarget(target);
    const suffixStart = fullTarget.search(/[?#]/);
    let pathname = suffixStart >= 0 ? fullTarget.slice(0, suffixStart) : fullTarget;
    const suffix = suffixStart >= 0 ? fullTarget.slice(suffixStart) : '';
    if (!pathname)
        pathname = '/';
    if (pathname !== '/' && pathname.endsWith('/')) {
        pathname = pathname.slice(0, -1);
        history.replaceState({}, '', pathname + suffix);
    }
    const fullURL = pathname + suffix;

    let handler = '';
    let pageURL = `${ASSET_PREFIX}pages/404.js`;
    try {
        handler = resolvePageHandler(pathname);
        // Module URLs use the on-disk `_slug` directory convention, not the
        // route-manifest `:slug` form — static hosts and native asset loaders
        // can only serve what exists on disk.
        const handlerPath = handler.replaceAll('/:', '/_');
        pageURL = handler === '/' ? `${ASSET_PREFIX}pages/tac.js` : `${ASSET_PREFIX}pages${handlerPath}/tac.js`;
    }
    catch {
        handler = '';
    }

    const wrapperPath = resolveWrapperPage(pathname);
    const wrapperChanged = wrapperPath !== currentWrapperPath;

    const loadPage = async () => {
        const pageFactory = await tac.load(pageURL);
        pageRender = await pageFactory();
        const pageHTML = await pageRender();
        if (containsIslandBoundary(pageHTML)) {
            navigateToIslandDocument(fullURL);
            return;
        }
        if (location.protocol !== 'file:') {
            if (location.pathname + location.search + location.hash !== fullURL)
                history.pushState({}, '', fullURL);
            else
                history.replaceState({}, '', fullURL);
        }
        currentPageURL = pageURL;
        freshNavigation = true;
        previousHTML = '';
        lazyLoaded.clear();
        lazyRenders.clear();
        try {
            if (wrapperRender) {
                patchSlot(pageHTML);
            }
            else {
                patchBody(pageHTML);
            }
        }
        finally {
            freshNavigation = false;
        }
    };

    if (wrapperPath && wrapperChanged) {
        Promise.all([
            tac.load(wrapperPath),
            tac.load(pageURL),
        ]).then(async ([wrapperFactory, pageFactory]) => {
            const nextWrapperRender = await wrapperFactory();
            const nextPageRender = await pageFactory();
            const wrapperHTML = await nextWrapperRender();
            const pageHTML = await nextPageRender();
            if (containsIslandBoundary(wrapperHTML) || containsIslandBoundary(pageHTML)) {
                navigateToIslandDocument(fullURL);
                return;
            }
            currentWrapperPath = wrapperPath;
            wrapperRender = nextWrapperRender;
            pageRender = nextPageRender;
            currentPageURL = pageURL;
            if (location.protocol !== 'file:') {
                if (location.pathname + location.search + location.hash !== fullURL)
                    history.pushState({}, '', fullURL);
                else
                    history.replaceState({}, '', fullURL);
            }
            freshNavigation = true;
            previousHTML = '';
            lazyLoaded.clear();
            lazyRenders.clear();
            previousHTML = pageHTML;
            const tempDoc = new DOMParser().parseFromString(`<body>${wrapperHTML}</body>`, 'text/html');
            const slot = tempDoc.getElementById('tc-page-slot');
            if (slot)
                slot.innerHTML = pageHTML;
            document.body.innerHTML = tempDoc.body.innerHTML;
            postPatch();
            freshNavigation = false;
        });
        return;
    }

    if (!wrapperPath && currentWrapperPath) {
        currentWrapperPath = null;
        wrapperRender = null;
        void loadPage();
        return;
    }

    void loadPage();
}

/** @param {string} pathname */
function resolvePageHandler(pathname) {
    const handler = resolveHandler(pathname, /** @type {any} */ (routes), slugs);
    if (handler === '/')
        return handler;
    const segments = pathname.split('/').slice(1);
    const routeSegments = handler.split('/').slice(1);
    params = parseParams(segments.slice(routeSegments.length));
    return handler;
}

/**
 * @param {URL} url
 * @returns {boolean}
 */
function canHandleClientNavigation(url) {
    try {
        resolvePageHandler(url.pathname);
        return true;
    }
    catch {
        return false;
    }
}

Object.assign(tac, {
    navigate,
    rerender,
});

// Reactive refresh entry point. A component instance passes its host id so its
// state change re-renders only its own subtree; the page root (no host id) and
// non-scopable hosts fall back to a full page refresh.
/** @param {string} [hostId] */
window.__tc_rerender = (hostId) => enqueueRenderPass(() => {
    if (hostId) {
        const host = document.getElementById(hostId);
        const entry = componentRegistry.scopable(hostId);
        if (host && entry)
            return rerenderComponent(host, entry, null);
    }
    return refreshCurrentView();
});

// ── HMR soft-reload ───────────────────────────────────────────────────
// Called by hot-reload-client.js instead of location.reload(). Invalidates
// the Tac module cache, re-imports fresh modules from the server, and
// re-renders the page in-place — no browser flash, scroll position preserved.
let __tc_hmr_generation = 0;
const __tc_original_tac_load = tac.load;
tac.load = async (/** @type {string} */ modulePath) => {
    if (__tc_hmr_generation > 0 && tac.modules.has(modulePath)) {
        tac.modules.delete(modulePath);
    }
    const busted = __tc_hmr_generation > 0
        ? (modulePath.includes('?') ? `${modulePath}&__hmr=${__tc_hmr_generation}` : `${modulePath}?__hmr=${__tc_hmr_generation}`)
        : modulePath;
    return __tc_original_tac_load(busted);
};

// ── Service worker registration (production only) ─────────────────────
// Preview hosts must stay network-authoritative. Production registrations
// bypass the HTTP cache when checking for a newer worker.
if (typeof navigator !== 'undefined'
    && 'serviceWorker' in navigator
    && ServiceWorkerPolicy.shouldRegister(location)) {
    const version = document.querySelector('meta[name="tachyon-version"]')?.getAttribute('content') || 'v1';
    navigator.serviceWorker.register(`${ASSET_PREFIX}tachyon-sw.js?v=${version}`, {
        // The built worker is an ES module (it imports the shared policy chunk).
        type: 'module',
        updateViaCache: 'none',
    }).then((registration) => registration.update()).catch(() => {
        // Service worker registration is best-effort; the app works without it.
    });
}

// Targeted HMR: re-import only the changed modules and re-render the current
// view in place. Falls back to a full soft-reload when the changed set
// includes the current page or wrapper module — their render closures are
// cached and would otherwise stay stale — or when nothing is rendered yet.
window.__tachyon_hmr_update__ = async (paths) => {
    if (!pageRender
        || (currentPageURL !== null && paths.includes(currentPageURL))
        || (currentWrapperPath !== null && paths.includes(currentWrapperPath))) {
        await fullSoftReload();
        return;
    }
    __tc_hmr_generation++;
    const scrollX = window.scrollX;
    const scrollY = window.scrollY;
    // Invalidate only the changed modules — everything else stays cached. The
    // cache-busting tac.load wrapper re-imports them on the next render.
    for (const p of paths)
        tac.modules.delete(p);
    lazyLoaded.clear();
    lazyRenders.clear();
    // Re-render the current view in place using the existing render closures;
    // their child components re-import fresh through the busted tac.load.
    await refreshCurrentView();
    requestAnimationFrame(() => { window.scrollTo(scrollX, scrollY); });
};

// Full soft-reload — clear the entire Tac module cache and re-navigate. Backs
// the legacy `reload` HMR signal and the targeted-update fallback above.
async function fullSoftReload() {
    __tc_hmr_generation++;
    const scrollX = window.scrollX;
    const scrollY = window.scrollY;
    tac.modules.clear();
    pageRender = null;
    wrapperRender = null;
    currentWrapperPath = null;
    currentPageURL = null;
    previousHTML = '';
    lazyLoaded.clear();
    lazyRenders.clear();
    delegatedEvents.clear();
    // Clear the Cache API so stale static assets are evicted
    clearStaticCache();
    await loadManifests();
    navigate(currentNavigationTarget());
    requestAnimationFrame(() => { window.scrollTo(scrollX, scrollY); });
}
window.__tachyon_hmr_reload__ = fullSoftReload;
