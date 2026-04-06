import {
  cleanBooleanAttrs,
  findEventTarget,
  morphChildren,
  parseFragment,
  parseParams,
  resolveHandler,
} from './dom-helpers.js';

type RenderFn = (elementId?: string | null, eventDetail?: unknown) => Promise<string>;

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

// ── Bootstrap ──────────────────────────────────────────────────────────────────
Promise.all([
  fetch('/routes.json').then(r => r.json()),
  fetch('/layouts.json').then(r => r.json()),
]).then(([routeData, layoutData]) => {
  for (const [path, s] of Object.entries(routeData))
    routes.set(path, s as Record<string, number>);
  Object.assign(layouts, layoutData);
  navigate(location.pathname);
});

// ── Event Delegation ───────────────────────────────────────────────────────────
// Single delegated listener at the document level instead of per-element binding.
// Handles both `@event` attribute actions and `:value` two-way binding.
document.addEventListener('click', (ev: MouseEvent) => {
  // SPA link interception
  const anchor = (ev.target as Element)?.closest('a[href]') as HTMLAnchorElement | null;
  if (anchor) {
    const url = new URL(anchor.href, location.origin);
    if (url.origin === location.origin) {
      ev.preventDefault();
      navigate(url.pathname);
      return;
    }
  }

  // Delegated @click
  const target = findEventTarget(ev.target as Element, 'click');
  if (target) {
    ev.preventDefault();
    dispatchAction(target);
  }
});

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

function dispatchAction(el: Element) {
  rerender(el.id);
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
    await render(triggerId, eventDetail);
    const html = await render();
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
    const mod = await import(modulePath);
    const render = await mod.default(props);
    lazyRenders.set(el.id, render);
    el.innerHTML = await render();
    el.removeAttribute('data-lazy-component');
    el.removeAttribute('data-lazy-path');
    el.removeAttribute('data-lazy-props');

    // Wire up event delegation for lazy-loaded content
    const eventEls = el.querySelectorAll('[\\@click]');
    // Events are already handled by delegation — no extra wiring needed
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
  observeLazyComponents();
  if (focusTarget) {
    const el = document.getElementById(focusTarget);
    if (el) try { el.focus(); } catch {}
    focusTarget = null;
  }
  freshNavigation = false;
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
    const mod = await import(pageURL);
    if (location.pathname !== pathname) history.pushState({}, '', pathname);
    else history.replaceState({}, '', pathname);
    pageRender = await mod.default();
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
      import(layoutPath),
      import(pageURL),
    ]).then(async ([layoutMod, pageMod]) => {
      currentLayoutPath = layoutPath;
      layoutRender = await layoutMod.default();

      if (location.pathname !== pathname) history.pushState({}, '', pathname);
      else history.replaceState({}, '', pathname);
      pageRender = await pageMod.default();

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
