// @ts-check
import { cleanBooleanAttrs, findEventTarget, morphChildren, parseFragment, parseParams, resolveHandler } from './dom-helpers.js';

/**
 * @typedef {{ path: string, allowSelf: boolean }} LayoutEntry
 * @typedef {Record<string, LayoutEntry>} LayoutManifest
 * @typedef {Record<string, Record<string, string>>} RouteManifest
 * @typedef {(elemId?: string | null, event?: unknown, compId?: string | null) => Promise<string>} TacRender
 * @typedef {(props?: unknown) => Promise<TacRender> | TacRender} TacFactory
 * @typedef {{ version: string, modules: Map<string, TacFactory>, register: (path: string, factory: TacFactory) => TacFactory, load: (path: string) => Promise<TacFactory> }} TacGlobal
 */

/** @returns {TacGlobal} */
function getTacGlobal() {
    const existing = /** @type {TacGlobal | undefined} */ (window.Tac);
    if (existing?.modules
        && typeof existing.register === 'function'
        && typeof existing.load === 'function') {
        return /** @type {TacGlobal} */ (existing);
    }

    /** @type {Map<string, TacFactory>} */
    const modules = existing?.modules ?? new Map();
    /** @type {TacGlobal} */
    const tac = {
        version: '1',
        modules,
        register(modulePath, factory) {
            modules.set(modulePath, factory);
            return factory;
        },
        async load(modulePath) {
            const registered = modules.get(modulePath);
            if (registered)
                return registered;
            const mod = await import(modulePath);
            if (typeof mod.default === 'function')
                return /** @type {TacFactory} */ (mod.default);
            const loaded = modules.get(modulePath);
            if (loaded)
                return loaded;
            throw new Error(`Tac module "${modulePath}" did not export or register a renderer`);
        },
    };

    const registry = /** @type {TacGlobal} */ (Object.assign(existing ?? {}, tac));
    window.Tac = /** @type {any} */ (registry);
    return registry;
}

/** @type {Map<string, unknown>} */
const context = new Map();
window.__ty_context__ = context;

/** @type {TacRender | null} */
let pageRender = null;
/** @type {TacRender | null} */
let layoutRender = null;
/** @type {string | null} */
let currentLayoutPath = null;
let previousHTML = '';
/** @type {string | null} */
let focusTarget = null;
let freshNavigation = false;

/** @type {Map<string, Record<string, string>>} */
const routes = new Map();
/** @type {LayoutManifest} */
const layouts = {};
/** @type {Record<string, string>} */
const slugs = {};
/** @type {unknown[]} */
let params = [];
const tac = getTacGlobal();
const delegatedEvents = new Set();

/**
 * Static HTML is rendered before the browser can read sessionStorage. Patch
 * marked persistent text nodes immediately so `$` fields do not visibly reset
 * while the full Tac route hydration is still loading manifests/modules.
 */
function prehydratePersistentText() {
    try {
        const nodes = document.querySelectorAll('[data-tac-persist-field]');
        for (const node of nodes) {
            if (!(node instanceof HTMLElement))
                continue;
            const fieldName = node.dataset.tacPersistField;
            const scope = node.closest('[data-tac-module]');
            if (!(scope instanceof HTMLElement) || !fieldName)
                continue;
            const modulePath = scope.dataset.tacModule;
            if (!modulePath || !scope.id)
                continue;
            const stored = sessionStorage.getItem(`tac:${modulePath}:${scope.id}:${fieldName}`);
            if (stored === null)
                continue;
            const value = JSON.parse(stored);
            node.textContent = value === null || value === undefined ? '' : String(value);
        }
    }
    catch { }
}

prehydratePersistentText();

async function loadManifests() {
    const [routeData, layoutData] = await Promise.all([
        fetch('/routes.json').then((response) => /** @type {Promise<RouteManifest>} */ (response.json())),
        fetch('/shells.json').then((response) => /** @type {Promise<LayoutManifest>} */ (response.json())),
    ]);

    routes.clear();
    for (const [routePath, routeConfig] of Object.entries(routeData)) {
        routes.set(routePath, routeConfig);
    }

    for (const key of Object.keys(layouts))
        delete layouts[key];
    Object.assign(layouts, layoutData);
}

Promise.all([loadManifests()]).then(() => {
    navigate(location.pathname);
});

/** @param {string} eventName */
function ensureDelegatedEvent(eventName) {
    if (!eventName || delegatedEvents.has(eventName))
        return;
    delegatedEvents.add(eventName);
    document.addEventListener(eventName, (event) => handleDelegatedEvent(eventName, event));
}

/**
 * @param {string} eventName
 * @param {Event} event
 */
function handleDelegatedEvent(eventName, event) {
    const eventTarget = event.target instanceof Element ? event.target : null;
    if (eventName === 'click') {
        const anchor = /** @type {HTMLAnchorElement | null} */ (eventTarget?.closest('a[href]'));
        if (anchor) {
            const url = new URL(anchor.href, location.origin);
            const hasModifierKey = event instanceof MouseEvent
                && (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey);
            const isPrimaryClick = !(event instanceof MouseEvent) || event.button === 0;
            const target = anchor.getAttribute('target');
            const allowSpaNavigation = url.origin === location.origin
                && !anchor.hasAttribute('download')
                && !hasModifierKey
                && isPrimaryClick
                && (!target || target === '_self')
                && canHandleClientNavigation(url);
            if (allowSpaNavigation) {
                event.preventDefault();
                navigate(url.pathname);
                return;
            }
        }
    }

    const target = findEventTarget(eventTarget, eventName);
    if (!target)
        return;

    if (eventName === 'click' || eventName === 'submit')
        event.preventDefault();
    dispatchAction(target, event);
}

/** @param {Element | ParentNode} [root] */
function registerDeclarativeEvents(root = document.body) {
    const elements = root instanceof Element
        ? [root, ...Array.from(root.querySelectorAll('*'))]
        : Array.from(root.querySelectorAll('*'));

    for (const element of elements) {
        for (const attr of Array.from(element.attributes)) {
            if (attr.name.startsWith('@'))
                ensureDelegatedEvent(attr.name.slice(1));
        }
    }
}

ensureDelegatedEvent('click');

for (const eventName of ['input', 'change', 'sl-input', 'sl-change']) {
    document.addEventListener(eventName, (event) => {
        const element = event.target instanceof Element ? event.target : null;
        if (!element || !element.id || !element.hasAttribute('value'))
            return;
        const value = 'value' in element ? element.value : undefined;
        rerender(element.id, { value });
    });
}

window.addEventListener('popstate', () => navigate(location.pathname));

/**
 * @param {Element} element
 * @param {unknown} eventDetail
 */
function dispatchAction(element, eventDetail) {
    rerender(element.id, eventDetail);
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
 * @param {string} triggerId
 * @param {unknown} [eventDetail]
 */
async function rerender(triggerId, eventDetail) {
    focusTarget = triggerId;
    const lazyContainer = findLazyAncestor(triggerId);
    if (lazyContainer) {
        const render = lazyRenders.get(lazyContainer.id);
        if (!render)
            return;
        await render(triggerId, eventDetail, lazyContainer.id);
        const html = await render(null, undefined, lazyContainer.id);
        morphChildren(lazyContainer, parseFragment(html), {
            preserveElement: (element) => lazyRenders.has(element.id),
        });
        postPatch();
        return;
    }

    const inSlot = layoutRender ? isInsideSlot(triggerId) : false;
    if (layoutRender && inSlot) {
        if (!pageRender)
            return;
        await pageRender(triggerId, eventDetail);
        patchSlot(await pageRender());
        return;
    }

    if (layoutRender) {
        await rerenderLayout(triggerId, eventDetail);
        return;
    }

    if (!pageRender)
        return;
    await pageRender(triggerId, eventDetail);
    patchBody(await pageRender());
}

async function refreshCurrentView() {
    if (!pageRender)
        return;
    previousHTML = '';

    if (layoutRender) {
        const [layoutHTML, pageHTML] = await Promise.all([
            layoutRender(),
            pageRender(),
        ]);
        const tempDoc = new DOMParser().parseFromString(`<body>${layoutHTML}</body>`, 'text/html');
        const slot = tempDoc.getElementById('ty-layout-slot');
        if (slot)
            slot.innerHTML = pageHTML;
        morphChildren(document.body, parseFragment(tempDoc.body.innerHTML), {
            preserveElement: (element) => lazyRenders.has(element.id),
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
        if (node.id === 'ty-layout-slot')
            return true;
        node = node.parentElement;
    }
    return false;
}

/**
 * @param {string} [triggerId]
 * @param {unknown} [eventDetail]
 */
async function rerenderLayout(triggerId, eventDetail) {
    if (!layoutRender)
        return;
    const slotHTML = document.getElementById('ty-layout-slot')?.innerHTML ?? '';
    if (triggerId)
        await layoutRender(triggerId, eventDetail);
    const html = await layoutRender();
    document.body.innerHTML = html;
    const slot = document.getElementById('ty-layout-slot');
    if (slot)
        slot.innerHTML = slotHTML;
    postPatch();
}

/** @param {string} pathname */
function resolveLayout(pathname) {
    if (pathname !== '/') {
        const segments = pathname.split('/');
        for (let i = segments.length; i >= 1; i -= 1) {
            const prefix = segments.slice(0, i).join('/') || '/';
            const entry = layouts[prefix];
            if (entry && (entry.allowSelf || prefix !== pathname))
                return entry.path;
        }
    }

    const rootEntry = layouts['/'];
    if (rootEntry && (rootEntry.allowSelf || pathname !== '/'))
        return rootEntry.path;
    return null;
}

/** @param {string} html */
function patchSlot(html) {
    const slot = document.getElementById('ty-layout-slot');
    if (!slot || (!html && !previousHTML) || html === previousHTML)
        return;
    previousHTML = html;
    if (freshNavigation) {
        slot.innerHTML = html;
    }
    else {
        morphChildren(slot, parseFragment(html), {
            preserveElement: (element) => lazyRenders.has(element.id),
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
            preserveElement: (element) => lazyRenders.has(element.id),
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
    registerDeclarativeEvents();
    observeLazyComponents();

    if (focusTarget) {
        const element = document.getElementById(focusTarget);
        if (element) {
            try {
                element.focus();
            }
            catch { }
        }
        focusTarget = null;
    }

    const queue = /** @type {Array<() => void | Promise<void>> | undefined} */ (window.__ty_onMount_queue__);
    if (queue?.length) {
        window.__ty_onMount_queue__ = [];
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
    }
    freshNavigation = false;
}

/** @param {string} pathname */
function navigate(pathname) {
    if (pathname !== '/' && pathname.endsWith('/')) {
        pathname = pathname.slice(0, -1);
        history.replaceState({}, '', pathname);
    }

    let handler = '';
    let pageURL = '/pages/404.js';
    try {
        handler = resolvePageHandler(pathname);
        pageURL = handler === '/' ? '/pages/index.js' : `/pages${handler}/index.js`;
    }
    catch {
        handler = '';
    }

    const layoutPath = resolveLayout(pathname);
    const layoutChanged = layoutPath !== currentLayoutPath;

    const loadPage = async () => {
        const pageFactory = await tac.load(pageURL);
        if (location.pathname !== pathname)
            history.pushState({}, '', pathname);
        else
            history.replaceState({}, '', pathname);
        pageRender = await pageFactory();
        freshNavigation = true;
        previousHTML = '';
        lazyLoaded.clear();
        lazyRenders.clear();
        try {
            if (layoutRender && pageRender) {
                patchSlot(await pageRender());
            }
            else if (pageRender) {
                patchBody(await pageRender());
            }
        }
        finally {
            freshNavigation = false;
        }
    };

    if (layoutPath && layoutChanged) {
        Promise.all([
            tac.load(layoutPath),
            tac.load(pageURL),
        ]).then(async ([layoutFactory, pageFactory]) => {
            currentLayoutPath = layoutPath;
            layoutRender = await layoutFactory();
            if (location.pathname !== pathname)
                history.pushState({}, '', pathname);
            else
                history.replaceState({}, '', pathname);
            pageRender = await pageFactory();
            freshNavigation = true;
            previousHTML = '';
            lazyLoaded.clear();
            lazyRenders.clear();
            const layoutHTML = await layoutRender();
            const pageHTML = await pageRender();
            previousHTML = pageHTML;
            const tempDoc = new DOMParser().parseFromString(`<body>${layoutHTML}</body>`, 'text/html');
            const slot = tempDoc.getElementById('ty-layout-slot');
            if (slot)
                slot.innerHTML = pageHTML;
            document.body.innerHTML = tempDoc.body.innerHTML;
            postPatch();
            freshNavigation = false;
        });
        return;
    }

    if (!layoutPath && currentLayoutPath) {
        currentLayoutPath = null;
        layoutRender = null;
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
    /** @param {string} key @param {unknown} value */
    provide: (key, value) => context.set(key, value),
});

window.__ty_rerender = refreshCurrentView;
