// @ts-check
/**
 * @typedef {string | number | boolean | null | undefined} ParamValue
 * @typedef {{ preserveElement?: (el: Element) => boolean }} MorphOptions
 */

/**
 * @param {Element | null} el
 * @param {string} eventName
 * @returns {Element | null}
 */
export function findEventTarget(el, eventName) {
    while (el && el !== document.body) {
        if (el.hasAttribute(`@${eventName}`))
            return el;
        el = el.parentElement;
    }
    return null;
}

/**
 * @param {string} html
 * @returns {DocumentFragment}
 */
export function parseFragment(html) {
    const doc = new DOMParser().parseFromString(`<body>${html}</body>`, 'text/html');
    const frag = document.createDocumentFragment();
    while (doc.body.firstChild)
        frag.appendChild(doc.body.firstChild);
    return frag;
}

    /**
     * @param {ParentNode} [root]
     */
export function cleanBooleanAttrs(root = document.body) {
    const all = root.querySelectorAll('*');
    for (const el of all) {
        for (const attr of Array.from(el.attributes)) {
            if (attr.name.endsWith('ed') && attr.value === 'false')
                el.removeAttribute(attr.name);
        }
    }
}

/**
 * @param {Node} a
 * @param {Node} b
 * @returns {boolean}
 */
export function isSameNode(a, b) {
    if (a.nodeType !== b.nodeType)
        return false;
    if (a.nodeType === Node.ELEMENT_NODE) {
        const ae = /** @type {Element} */ (a);
        const be = /** @type {Element} */ (b);
        if (ae.tagName !== be.tagName)
            return false;
        if (ae.id && be.id)
            return ae.id === be.id;
    }
    return true;
}

/**
 * Keeps live form-control state in sync with updated attributes.
 * @param {Element} oldEl
 * @param {Element} newEl
 */
function syncFormControlState(oldEl, newEl) {
    if (oldEl instanceof HTMLInputElement && newEl instanceof HTMLInputElement) {
        const nextValue = newEl.getAttribute('value') ?? newEl.value;
        if (oldEl.value !== nextValue)
            oldEl.value = nextValue;
        if (oldEl.checked !== newEl.checked)
            oldEl.checked = newEl.checked;
        return;
    }
    if (oldEl instanceof HTMLTextAreaElement && newEl instanceof HTMLTextAreaElement) {
        const nextValue = newEl.getAttribute('value') ?? newEl.value;
        if (oldEl.value !== nextValue)
            oldEl.value = nextValue;
        if (oldEl.textContent !== nextValue)
            oldEl.textContent = nextValue;
        return;
    }
    if (oldEl instanceof HTMLSelectElement && newEl instanceof HTMLSelectElement) {
        const nextValue = newEl.getAttribute('value') ?? newEl.value;
        if (oldEl.value !== nextValue)
            oldEl.value = nextValue;
    }
}

/**
 * @param {Element} oldEl
 * @param {Element} newEl
 */
export function syncAttributes(oldEl, newEl) {
    for (const attr of Array.from(oldEl.attributes)) {
        if (!attr.name.startsWith('@') && !newEl.hasAttribute(attr.name))
            oldEl.removeAttribute(attr.name);
    }
    for (const attr of Array.from(newEl.attributes)) {
        if (!attr.name.startsWith('@') && oldEl.getAttribute(attr.name) !== attr.value)
            oldEl.setAttribute(attr.name, attr.value);
    }
    syncFormControlState(oldEl, newEl);
}

/**
 * @param {Element | DocumentFragment} parent
 * @param {DocumentFragment} desired
 * @param {MorphOptions} [options]
 */
export function morphChildren(parent, desired, options = {}) {
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
        if (!oldChild || !newChild)
            continue;
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
            const oldElement = /** @type {Element} */ (oldChild);
            if (options.preserveElement?.(oldElement))
                continue;
            syncAttributes(oldElement, /** @type {Element} */ (newChild));
            const childFrag = document.createDocumentFragment();
            while (newChild.firstChild)
                childFrag.appendChild(newChild.firstChild);
            morphChildren(oldElement, childFrag, options);
        }
    }
    while (parent.childNodes.length > newNodes.length) {
        parent.removeChild(/** @type {ChildNode} */ (parent.lastChild));
    }
}

/**
 * @param {string[]} input
 * @returns {ParamValue[]}
 */
export function parseParams(input) {
    return input.map(p => {
        const n = Number(p);
        if (!Number.isNaN(n))
            return n;
        if (p === 'true')
            return true;
        if (p === 'false')
            return false;
        if (p === 'null')
            return null;
        if (p === 'undefined')
            return undefined;
        return p;
    });
}

/**
 * @param {string} pathname
 * @param {Map<string, Record<string, number>>} routes
 * @param {Record<string, string>} slugs
 * @returns {string}
 */
export function resolveHandler(pathname, routes, slugs) {
    if (pathname === '/')
        return pathname;
    const segments = pathname.split('/').slice(1);
    let bestKey = '';
    let bestLen = -1;
    for (const [routeKey] of routes) {
        const routeSegs = routeKey.split('/').slice(1);
        if (routeSegs.length > segments.length)
            continue;
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
    if (!bestKey)
        throw new Error(`Route ${pathname} not found`);
    const slugMap = routes.get(bestKey) ?? {};
    for (const [key, idx] of Object.entries(slugMap)) {
        slugs[key.replace(':', '')] = segments[idx];
    }
    return bestKey;
}
