export function findEventTarget(el: Element | null, eventName: string): Element | null {
  while (el && el !== document.body) {
    if (el.hasAttribute(`@${eventName}`)) return el;
    el = el.parentElement;
  }
  return null;
}

export function parseFragment(html: string): DocumentFragment {
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, 'text/html');
  const frag = document.createDocumentFragment();
  while (doc.body.firstChild) frag.appendChild(doc.body.firstChild);
  return frag;
}

export function cleanBooleanAttrs(root: ParentNode = document.body) {
  const all = root.querySelectorAll('*');
  for (const el of all) {
    for (const attr of Array.from(el.attributes)) {
      if (attr.name.endsWith('ed') && attr.value === 'false')
        el.removeAttribute(attr.name);
    }
  }
}

export function isSameNode(a: Node, b: Node): boolean {
  if (a.nodeType !== b.nodeType) return false;
  if (a.nodeType === Node.ELEMENT_NODE) {
    const ae = a as Element;
    const be = b as Element;
    if (ae.tagName !== be.tagName) return false;
    if (ae.id && be.id) return ae.id === be.id;
  }
  return true;
}

export function syncAttributes(oldEl: Element, newEl: Element) {
  for (const attr of Array.from(oldEl.attributes)) {
    if (!attr.name.startsWith('@') && !newEl.hasAttribute(attr.name))
      oldEl.removeAttribute(attr.name);
  }

  for (const attr of Array.from(newEl.attributes)) {
    if (!attr.name.startsWith('@') && oldEl.getAttribute(attr.name) !== attr.value)
      oldEl.setAttribute(attr.name, attr.value);
  }
}

export function morphChildren(
  parent: Element | DocumentFragment,
  desired: DocumentFragment,
  options: { preserveElement?: (el: Element) => boolean } = {}
) {
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

    if (oldChild.nodeType === Node.TEXT_NODE) {
      if (oldChild.textContent !== newChild.textContent)
        oldChild.textContent = newChild.textContent;
      continue;
    }

    if (oldChild.nodeType === Node.ELEMENT_NODE) {
      const oldElement = oldChild as Element;
      if (options.preserveElement?.(oldElement)) continue;

      syncAttributes(oldElement, newChild as Element);
      const childFrag = document.createDocumentFragment();
      while (newChild.firstChild) childFrag.appendChild(newChild.firstChild);
      morphChildren(oldElement, childFrag, options);
    }
  }

  while (parent.childNodes.length > newNodes.length) {
    parent.removeChild(parent.lastChild!);
  }
}

export function parseParams(input: string[]) {
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

export function resolveHandler(
  pathname: string,
  routes: Map<string, Record<string, number>>,
  slugs: Record<string, string>
): string {
  if (pathname === '/') return pathname;

  const segments = pathname.split('/').slice(1);
  let bestKey = '';
  let bestLen = -1;

  for (const [routeKey] of routes) {
    const routeSegs = routeKey.split('/').slice(1);
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

  const slugMap = routes.get(bestKey) ?? {};
  for (const [key, idx] of Object.entries(slugMap)) {
    slugs[key.replace(':', '')] = segments[idx];
  }

  return bestKey;
}
