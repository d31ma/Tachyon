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

const parser = new DOMParser();

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

// ── Event helpers ──────────────────────────────────────────────────────────────
function findEventTarget(el: Element | null, eventName: string): Element | null {
  while (el && el !== document.body) {
    if (el.hasAttribute(`@${eventName}`)) return el;
    el = el.parentElement;
  }
  return null;
}

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
    morphChildren(lazyContainer, parseFragment(html));
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
    morphChildren(slot, parseFragment(html));
  }
  postPatch();
}

function patchBody(html: string) {
  if (!html || html === previousHTML) return;
  previousHTML = html;

  if (freshNavigation) {
    document.body.innerHTML = html;
  } else {
    morphChildren(document.body, parseFragment(html));
  }
  postPatch();
}

function parseFragment(html: string): DocumentFragment {
  const doc = parser.parseFromString(`<body>${html}</body>`, 'text/html');
  const frag = document.createDocumentFragment();
  while (doc.body.firstChild) frag.appendChild(doc.body.firstChild);
  return frag;
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
  const props = el.dataset.lazyProps || '';

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

/** Remove boolean HTML attributes that are explicitly "false" */
function cleanBooleanAttrs() {
  const all = document.body.querySelectorAll('*');
  for (const el of all) {
    for (const attr of Array.from(el.attributes)) {
      if (attr.name.endsWith('ed') && attr.value === 'false')
        el.removeAttribute(attr.name);
    }
  }
}

// ── DOM Morphing ───────────────────────────────────────────────────────────────
// Efficient keyed reconciliation: matches nodes by id, then by tag+position.
function morphChildren(parent: Element | DocumentFragment, desired: DocumentFragment) {
  const oldNodes = Array.from(parent.childNodes);
  const newNodes = Array.from(desired.childNodes);

  const maxLen = Math.max(oldNodes.length, newNodes.length);

  for (let i = 0; i < maxLen; i++) {
    const oldChild = oldNodes[i];
    const newChild = newNodes[i];

    if (!oldChild && newChild) {
      parent.appendChild(newChild.cloneNode(true));
      continue;
    }
    if (oldChild && !newChild) {
      parent.removeChild(oldChild);
      continue;
    }
    if (!oldChild || !newChild) continue;

    if (!isSameNode(oldChild, newChild)) {
      parent.replaceChild(newChild.cloneNode(true), oldChild);
      continue;
    }

    // Text nodes
    if (oldChild.nodeType === Node.TEXT_NODE) {
      if (oldChild.textContent !== newChild.textContent)
        oldChild.textContent = newChild.textContent;
      continue;
    }

    // Element nodes — sync attributes then recurse
    if (oldChild.nodeType === Node.ELEMENT_NODE) {
      // Preserve lazy-loaded components — the page render outputs an empty placeholder
      // but the live DOM has the loaded component content
      if (lazyRenders.has((oldChild as Element).id)) continue;

      syncAttributes(oldChild as Element, newChild as Element);
      // Convert newChild children to a fragment for recursion
      const childFrag = document.createDocumentFragment();
      while (newChild.firstChild) childFrag.appendChild(newChild.firstChild);
      morphChildren(oldChild as Element, childFrag);
    }
  }

  // Trim excess old nodes
  while (parent.childNodes.length > newNodes.length) {
    parent.removeChild(parent.lastChild!);
  }
}

function isSameNode(a: Node, b: Node): boolean {
  if (a.nodeType !== b.nodeType) return false;
  if (a.nodeType === Node.ELEMENT_NODE) {
    const ae = a as Element, be = b as Element;
    if (ae.tagName !== be.tagName) return false;
    // Prefer matching by id for keyed reconciliation
    if (ae.id && be.id) return ae.id === be.id;
  }
  return true;
}

function syncAttributes(oldEl: Element, newEl: Element) {
  // Remove stale attributes (skip event attributes — they're declarative)
  for (const attr of Array.from(oldEl.attributes)) {
    if (!attr.name.startsWith('@') && !newEl.hasAttribute(attr.name))
      oldEl.removeAttribute(attr.name);
  }
  // Add/update attributes
  for (const attr of Array.from(newEl.attributes)) {
    if (!attr.name.startsWith('@') && oldEl.getAttribute(attr.name) !== attr.value)
      oldEl.setAttribute(attr.name, attr.value);
  }
}

// ── Navigation / Routing ───────────────────────────────────────────────────────
function navigate(pathname: string) {
  let handler: string;
  let pageURL: string;

  try {
    handler = resolveHandler(pathname);
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
    import(layoutPath).then(async (mod) => {
      currentLayoutPath = layoutPath;
      layoutRender = await mod.default();
      freshNavigation = true;
      previousHTML = '';
      try {
        document.body.innerHTML = await layoutRender!();
      } finally {
        freshNavigation = false;
      }
      await loadPage();
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

function resolveHandler(pathname: string): string {
  if (pathname === '/') return pathname;

  const segments = pathname.split('/').slice(1);
  let bestKey = '';
  let bestLen = -1;

  for (const [routeKey] of routes) {
    const routeSegs = routeKey.split('/');
    if (routeSegs.length > segments.length) continue;

    const slugMap = routes.get(routeKey) ?? {};
    let match = true;

    for (let i = 0; i < routeSegs.length; i++) {
      if (!slugMap[routeSegs[i]] && routeSegs[i] !== segments[i]) {
        match = false;
        break;
      }
    }

    if (match && routeSegs.length > bestLen) {
      bestKey = routeKey;
      bestLen = routeSegs.length;
    }
  }

  if (!bestKey) throw new Error(`Route ${pathname} not found`);

  // Set slugs and params
  const slugMap = routes.get(bestKey) ?? {};
  for (const [key, idx] of Object.entries(slugMap)) {
    slugs[key.replace(':', '')] = segments[idx];
  }
  params = parseParams(segments.slice(bestLen));

  return bestKey;
}

function parseParams(input: string[]) {
  return input.map(p => {
    const n = Number(p);
    if (!Number.isNaN(n)) return n;
    if (p === 'true') return true;
    if (p === 'false') return false;
    if (p === 'null') return null;
    if (p === 'undefined') return undefined;
    return p;
  });
}
