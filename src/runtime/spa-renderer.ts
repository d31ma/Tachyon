import {
  cleanBooleanAttrs,
  findEventTarget,
  morphChildren,
  parseFragment,
  parseParams,
  resolveHandler,
} from './dom-helpers.js';

type RenderFn = (elementId?: string | null, eventDetail?: unknown, componentRootId?: string | null) => Promise<string>;
type RenderFactory = (props?: unknown) => Promise<RenderFn>;
type YonGlobal = {
  version: string;
  modules: Map<string, RenderFactory>;
  register(path: string, factory: RenderFactory): RenderFactory;
  load(path: string): Promise<RenderFactory>;
  navigate?: (pathname: string) => void;
  rerender?: (triggerId: string, eventDetail?: unknown) => Promise<void>;
  provide?: (key: string, value: unknown) => void;
};

declare global {
  interface Window {
    Yon?: YonGlobal;
    __ty_rerender?: () => Promise<void>;
    __ty_onMount_queue__?: Array<() => void>;
    __ty_context__?: Map<string, unknown>;
  }
}

function getYonGlobal(): YonGlobal {
  const existing = window.Yon;
  if (
    existing?.modules
    && typeof existing.register === 'function'
    && typeof existing.load === 'function'
  ) return existing;

  const modules = existing?.modules ?? new Map<string, RenderFactory>();
  const yon: YonGlobal = {
    version: '1',
    modules,
    register(path, factory) {
      modules.set(path, factory);
      return factory;
    },
    async load(path) {
      const registered = modules.get(path);
      if (registered) return registered;

      const mod = await import(path);
      if (typeof mod.default === 'function') return mod.default as RenderFactory;

      const loaded = modules.get(path);
      if (loaded) return loaded;

      throw new Error(`Yon module "${path}" did not export or register a renderer`);
    },
  };

  window.Yon = Object.assign(existing ?? {}, yon);
  return window.Yon;
}

// ── Context ────────────────────────────────────────────────────────────────────
const context = new Map<string, unknown>();
window.__ty_context__ = context;

// ── State ──────────────────────────────────────────────────────────────────────
let pageRender: RenderFn;
let layoutRender: RenderFn | null = null;
let currentLayoutPath: string | null = null;
let previousHTML = '';
let focusTarget: string | null = null;
let freshNavigation = false;

const routes = new Map<string, Record<string, number>>();
const layouts: Record<string, string> = {};
const slugs: Record<string, string> = {};
let params: (string | number | boolean | null | undefined)[] = [];
const yon = getYonGlobal();
const delegatedEvents = new Set<string>();

async function loadManifests() {
  const [routeData, layoutData] = await Promise.all([
    fetch('/routes.json').then(r => r.json()),
    fetch('/layouts.json').then(r => r.json()),
  ]);

  routes.clear();
  for (const [path, s] of Object.entries(routeData)) {
    routes.set(path, s as Record<string, number>);
  }

  for (const key of Object.keys(layouts)) delete layouts[key];
  Object.assign(layouts, layoutData);
}

// ── Bootstrap ──────────────────────────────────────────────────────────────────
Promise.all([
  loadManifests(),
]).then(() => {
  navigate(location.pathname);
});

// ── Event Delegation ───────────────────────────────────────────────────────────
// Single delegated listener per event name at the document level.
function ensureDelegatedEvent(eventName: string) {
  if (!eventName || delegatedEvents.has(eventName)) return;
  delegatedEvents.add(eventName);
  document.addEventListener(eventName, (ev: Event) => handleDelegatedEvent(eventName, ev));
}

function handleDelegatedEvent(eventName: string, ev: Event) {
  const eventTarget = ev.target instanceof Element ? ev.target : null;

  if (eventName === 'click') {
    // SPA link interception
    const anchor = eventTarget?.closest('a[href]') as HTMLAnchorElement | null;
    if (anchor) {
      const url = new URL(anchor.href, location.origin);
      if (url.origin === location.origin) {
        ev.preventDefault();
        navigate(url.pathname);
        return;
      }
    }
  }

  const target = findEventTarget(eventTarget, eventName);
  if (target) {
    if (eventName === 'click' || eventName === 'submit') ev.preventDefault();
    dispatchAction(target, ev);
  }
}

function registerDeclarativeEvents(root: ParentNode = document.body) {
  const elements = root instanceof Element
    ? [root, ...Array.from(root.querySelectorAll('*'))]
    : Array.from(root.querySelectorAll('*'));

  for (const el of elements) {
    for (const attr of Array.from(el.attributes)) {
      if (attr.name.startsWith('@')) ensureDelegatedEvent(attr.name.slice(1));
    }
  }
}

// Keep same-origin navigation active even on pages without an explicit @click.
ensureDelegatedEvent('click');

// Value-change events (input, change, sl-input, sl-change)
for (const eventName of ['input', 'change', 'sl-input', 'sl-change'] as const) {
  document.addEventListener(eventName, (ev: Event) => {
    const el = ev.target as HTMLElement;
    if (!el?.id || !el.hasAttribute('value')) return;
    const value = (el as HTMLInputElement).value;
    rerender(el.id, { value });
  });
}

window.addEventListener('popstate', () => navigate(location.pathname));

function dispatchAction(el: Element, eventDetail?: unknown) {
  rerender(el.id, eventDetail);
}

function findLazyAncestor(elementId: string): HTMLElement | null {
  let el = document.getElementById(elementId);
  while (el) {
    if (lazyRenders.has(el.id)) return el;
    el = el.parentElement;
  }
  return null;
}

async function rerender(triggerId: string, eventDetail?: unknown) {
  focusTarget = triggerId;

  // Check if the event is inside a lazy-loaded component
  const lazyContainer = findLazyAncestor(triggerId);
  if (lazyContainer) {
    const render = lazyRenders.get(lazyContainer.id) as RenderFn;
    await render(triggerId, eventDetail, lazyContainer.id);
    const html = await render(null, undefined, lazyContainer.id);
    morphChildren(lazyContainer, parseFragment(html), {
      preserveElement: (el) => lazyRenders.has(el.id)
    });
    postPatch();
    return;
  }

  const inSlot = layoutRender ? isInsideSlot(triggerId) : false;

  if (layoutRender && inSlot) {
    // First call executes the matched action (mutates state), second builds clean HTML
    await pageRender(triggerId, eventDetail);
    const html = await pageRender();
    patchSlot(html);
  } else if (layoutRender) {
    await rerenderLayout(triggerId, eventDetail);
  } else {
    await pageRender(triggerId, eventDetail);
    patchBody(await pageRender());
  }
}

async function refreshCurrentView() {
  if (!pageRender) return;

  previousHTML = '';

  if (layoutRender) {
    const [layoutHTML, pageHTML] = await Promise.all([
      layoutRender(),
      pageRender(),
    ]);
    const tempDoc = new DOMParser().parseFromString(`<body>${layoutHTML}</body>`, 'text/html');
    const slot = tempDoc.getElementById('ty-layout-slot');
    if (slot) slot.innerHTML = pageHTML;
    morphChildren(document.body, parseFragment(tempDoc.body.innerHTML), {
      preserveElement: (el) => lazyRenders.has(el.id)
    });
    previousHTML = pageHTML;
  } else {
    patchBody(await pageRender());
  }

  postPatch();
}

function isInsideSlot(elementId: string): boolean {
  const el = document.getElementById(elementId);
  if (!el) return false;
  let node: Element | null = el;
  while (node) {
    if (node.id === 'ty-layout-slot') return true;
    node = node.parentElement;
  }
  return false;
}

// ── Layout ─────────────────────────────────────────────────────────────────────
async function rerenderLayout(triggerId?: string | null, eventDetail?: unknown) {
  if (!layoutRender) return;
  const slotHTML = document.getElementById('ty-layout-slot')?.innerHTML ?? '';
  // First call executes the matched action, second builds clean HTML
  if (triggerId) await layoutRender(triggerId, eventDetail);
  const html = await layoutRender();
  document.body.innerHTML = html;
  const slot = document.getElementById('ty-layout-slot');
  if (slot) slot.innerHTML = slotHTML;
  postPatch();
}

function resolveLayout(pathname: string): string | null {
  if (pathname !== '/') {
    const segs = pathname.split('/');
    for (let i = segs.length; i >= 1; i--) {
      const prefix = segs.slice(0, i).join('/') || '/';
      if (layouts[prefix]) return layouts[prefix];
    }
  }
  return layouts['/'] ?? null;
}

// ── DOM Patching ───────────────────────────────────────────────────────────────
function patchSlot(html: string) {
  const slot = document.getElementById('ty-layout-slot');
  if (!slot || (!html && !previousHTML)) return;
  if (html === previousHTML) return;
  previousHTML = html;

  if (freshNavigation) {
    slot.innerHTML = html;
  } else {
    morphChildren(slot, parseFragment(html), {
      preserveElement: (el) => lazyRenders.has(el.id)
    });
  }
  postPatch();
}

function patchBody(html: string) {
  if (!html || html === previousHTML) return;
  previousHTML = html;

  if (freshNavigation) {
    document.body.innerHTML = html;
  } else {
    morphChildren(document.body, parseFragment(html), {
      preserveElement: (el) => lazyRenders.has(el.id)
    });
  }
  postPatch();
}

// ── Lazy Component Loading ─────────────────────────────────────────────────────
const lazyLoaded = new Set<string>();
const lazyRenders = new Map<string, Function>();

const lazyObserver = new IntersectionObserver((entries) => {
  for (const entry of entries) {
    if (!entry.isIntersecting) continue;
    const el = entry.target as HTMLElement;
    const id = el.id;
    if (lazyLoaded.has(id)) continue;
    lazyLoaded.add(id);
    lazyObserver.unobserve(el);
    loadLazyComponent(el);
  }
}, { rootMargin: '100px' });

async function loadLazyComponent(el: HTMLElement) {
  const path = el.dataset.lazyComponent!;
  const modulePath = el.dataset.lazyPath!;
  const propsRaw = el.dataset.lazyProps || '';
  const props = propsRaw ? JSON.parse(decodeURIComponent(propsRaw)) : null;

  try {
    const factory = await yon.load(modulePath);
    const render = await factory(props);
    lazyRenders.set(el.id, render);
    el.innerHTML = await render(null, undefined, el.id);
    el.removeAttribute('data-lazy-component');
    el.removeAttribute('data-lazy-path');
    el.removeAttribute('data-lazy-props');
    postPatch();
  } catch (e) {
    console.error(`[tachyon] Failed to load lazy component "${path}":`, e);
  }
}

function observeLazyComponents() {
  const placeholders = document.querySelectorAll('[data-lazy-component]');
  for (const el of placeholders) {
    if (!lazyLoaded.has(el.id)) {
      lazyObserver.observe(el);
    }
  }
}

function postPatch() {
  cleanBooleanAttrs();
  registerDeclarativeEvents();
  observeLazyComponents();
  if (focusTarget) {
    const el = document.getElementById(focusTarget);
    if (el) try { el.focus(); } catch {}
    focusTarget = null;
  }

  if (freshNavigation) {
    window.dispatchEvent(new CustomEvent('tachyon:navigate', { detail: { pathname: location.pathname } }));
  }
  freshNavigation = false;

  const queue = window.__ty_onMount_queue__;
  if (queue?.length) {
    window.__ty_onMount_queue__ = [];
    for (const fn of queue) {
      try { fn(); } catch (e) { console.error('[tachyon] onMount callback error:', e); }
    }
  }
}

// ── Navigation / Routing ───────────────────────────────────────────────────────
function navigate(pathname: string) {
  // Normalize trailing slash (e.g. Amplify 301s /docs → /docs/)
  if (pathname !== '/' && pathname.endsWith('/')) {
    pathname = pathname.slice(0, -1);
    history.replaceState({}, '', pathname);
  }

  let handler: string;
  let pageURL: string;

  try {
    handler = resolvePageHandler(pathname);
    pageURL = `/pages${handler === '/' ? '' : handler}/HTML.js`;
  } catch {
    pageURL = '/pages/404.js';
    handler = '';
  }

  const layoutPath = resolveLayout(pathname);
  const layoutChanged = layoutPath !== currentLayoutPath;

  const loadPage = async () => {
    const pageFactory = await yon.load(pageURL);
    if (location.pathname !== pathname) history.pushState({}, '', pathname);
    else history.replaceState({}, '', pathname);
    pageRender = await pageFactory();
    freshNavigation = true;
    previousHTML = '';
    lazyLoaded.clear();
    lazyRenders.clear();
    try {
      if (layoutRender) {
        patchSlot(await pageRender());
      } else {
        patchBody(await pageRender());
      }
    } finally {
      freshNavigation = false;
    }
  };

  if (layoutPath && layoutChanged) {
    Promise.all([
      yon.load(layoutPath),
      yon.load(pageURL),
    ]).then(async ([layoutFactory, pageFactory]) => {
      currentLayoutPath = layoutPath;
      layoutRender = await layoutFactory();

      if (location.pathname !== pathname) history.pushState({}, '', pathname);
      else history.replaceState({}, '', pathname);
      pageRender = await pageFactory();

      freshNavigation = true;
      previousHTML = '';
      lazyLoaded.clear();
      lazyRenders.clear();

      // Render layout and page content in one DOM write to eliminate CLS
      const layoutHTML = await layoutRender!();
      const pageHTML = await pageRender();
      previousHTML = pageHTML;

      const tempDoc = new DOMParser().parseFromString(`<body>${layoutHTML}</body>`, 'text/html');
      const slot = tempDoc.getElementById('ty-layout-slot');
      if (slot) slot.innerHTML = pageHTML;
      document.body.innerHTML = tempDoc.body.innerHTML;

      postPatch();
      freshNavigation = false;
    });
  } else if (!layoutPath && currentLayoutPath) {
    // Leaving a layout
    currentLayoutPath = null;
    layoutRender = null;
    loadPage();
  } else {
    loadPage();
  }
}


function resolvePageHandler(pathname: string): string {
  const handler = resolveHandler(pathname, routes, slugs);
  if (handler === '/') return handler;

  const segments = pathname.split('/').slice(1);
  const routeSegs = handler.split('/').slice(1);
  params = parseParams(segments.slice(routeSegs.length));
  return handler;
}

Object.assign(yon, {
  navigate,
  rerender,
  provide: (key: string, value: unknown) => context.set(key, value),
});

window.__ty_rerender = refreshCurrentView;
