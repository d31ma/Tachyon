// @ts-check
/**
 * @typedef {string | number | boolean | null | undefined} ParamValue
 * @typedef {{ preserveElement?: (element: Element) => boolean }} MorphOptions
 * @typedef {{ kind: 'text', value: string, checked?: boolean, selectionStart: number | null, selectionEnd: number | null, selectionDirection: 'forward' | 'backward' | 'none' | null }
 *   | { kind: 'select', selectedOptions: Array<{ id: string, value: string, label: string, index: number }> }
 *   | { kind: 'contenteditable', html: string, anchorOffset: number | null, focusOffset: number | null }} ActiveControlState
 */

/**
 * @param {Element | null} element
 * @param {string} eventName
 * @returns {Element | null}
 */
export function findEventTarget(element, eventName) {
    // Tac compiles `on<event>` to a DuVay-safe `data-tac-on-<event>` marker (colons
    // in component event names encoded to `__`).
    const marker = `data-tac-on-${eventName.replaceAll(':', '__')}`;
    let candidate = element;
    while (candidate && candidate !== document.body) {
        if (candidate.hasAttribute(marker))
            return candidate;
        candidate = candidate.parentElement;
    }
    return null;
}

/**
 * Finds the element that owns an in-app navigation request. Native anchors are
 * supported directly; custom elements can opt in by exposing an `href`
 * attribute, which is how DuVay's `<w-btn>` represents link buttons.
 *
 * @param {Element} element
 * @returns {boolean}
 */
function isNavigationTarget(element) {
    return element.hasAttribute('href')
        && !element.hasAttribute('disabled')
        && (element.localName === 'a' || element.localName.includes('-'));
}

/**
 * Resolves a navigation target across regular and shadow DOM. The composed
 * path makes native links rendered inside a web component visible; the host
 * fallback supports light-DOM components that expose `href` themselves.
 *
 * @param {Event} event
 * @returns {Element | null}
 */
export function findNavigationTarget(event) {
    if (typeof event.composedPath === 'function') {
        for (const node of event.composedPath()) {
            if (node instanceof Element && isNavigationTarget(node))
                return node;
        }
    }
    let candidate = event.target instanceof Element ? event.target : null;
    while (candidate && candidate !== document.body) {
        if (isNavigationTarget(candidate))
            return candidate;
        candidate = candidate.parentElement;
    }
    return null;
}

/**
 * Builds the event context used by the value-binding rerender path while
 * preserving DOM-style `$event.target.value` handler access.
 * @param {Element & { value?: unknown }} element
 * @param {Event} event
 * @returns {{ value: unknown, target: Element, currentTarget: Element, type: string }}
 */
export function createValueEventDetail(element, event) {
    return {
        value: 'value' in element ? element.value : undefined,
        target: element,
        currentTarget: element,
        type: event.type,
    };
}

/**
 * Captures a conditional claim on the element that owns an interaction. A
 * patch may replace that element and drop focus to the document body; in that
 * case the replacement should regain focus. If the user has focused anything
 * else while the async render was pending, that newer choice always wins.
 * @param {string} triggerId
 */
export function createFocusLease(triggerId) {
    const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const activeCompilerId = active?.getAttribute('data-tac-id') ?? '';
    const ownsTrigger = activeCompilerId
        ? activeCompilerId === triggerId
        : active?.id === triggerId;
    const activeId = ownsTrigger ? active?.id ?? '' : '';
    const activeTacId = ownsTrigger ? active?.getAttribute('data-tac-id') ?? '' : '';
    const liveState = ownsTrigger && active ? captureActiveControlState(active) : null;
    let invalidated = false;
    /** @param {FocusEvent} event */
    const invalidateOnFocusMove = (event) => {
        if (event.target !== active)
            invalidated = true;
    };
    if (ownsTrigger)
        document.addEventListener('focusin', invalidateOnFocusMove, true);
    return {
        restore() {
            if (ownsTrigger)
                document.removeEventListener('focusin', invalidateOnFocusMove, true);
            if (!ownsTrigger || invalidated)
                return;
            const current = document.activeElement;
            if (current && current !== document.body && current !== document.documentElement)
                return;
            /** @type {Element | null} */
            let replacement = null;
            if (activeTacId) {
                replacement = Array.from(document.querySelectorAll('[data-tac-id]'))
                    .find((element) => element.getAttribute('data-tac-id') === activeTacId) ?? null;
            }
            else if (activeId) {
                replacement = document.getElementById(activeId);
                if (replacement?.hasAttribute('data-tac-id'))
                    replacement = null;
            }
            const compatible = active instanceof HTMLElement
                && replacement instanceof HTMLElement
                && canTransferEditorState(active, replacement);
            if (compatible && replacement instanceof HTMLElement) {
                try {
                    replacement.focus();
                    restoreActiveControlState(replacement, liveState);
                }
                catch {
                    // A replacement can become non-focusable; preserving the patch is enough.
                }
            }
        },
    };
}

/**
 * Re-points an event's `currentTarget` to the element a delegated handler is
 * bound to. Tac delegation listens on `document` and defers dispatch to a
 * microtask, so the browser has already reset `currentTarget` (to document/null)
 * by the time the handler runs. Overriding the instance property restores native
 * addEventListener semantics — `currentTarget` is the handler's element — while
 * leaving `target` (the origin element) intact. No-ops on frozen/exotic events.
 * @param {unknown} event
 * @param {Element} element
 */
export function repointCurrentTarget(event, element) {
    if (!event || typeof event !== 'object')
        return;
    try {
        Object.defineProperty(event, 'currentTarget', { configurable: true, value: element });
    }
    catch {
        // Non-configurable/frozen event — handlers can still use event.target.
    }
}

/**
 * @param {string} html
 * @returns {DocumentFragment}
 */
export function parseFragment(html) {
    const parsedDocument = new DOMParser().parseFromString(`<body>${html}</body>`, 'text/html');
    const fragment = document.createDocumentFragment();
    while (parsedDocument.body.firstChild)
        fragment.appendChild(parsedDocument.body.firstChild);
    return fragment;
}

/**
 * @param {ParentNode} [root]
 */
export function cleanBooleanAttrs(root = document.body) {
    const elements = root.querySelectorAll('*');
    for (const element of elements) {
        for (const attribute of Array.from(element.attributes)) {
            if (attribute.name.endsWith('ed') && attribute.value === 'false')
                element.removeAttribute(attribute.name);
        }
    }
}

/**
 * @param {Node} currentNode
 * @param {Node} desiredNode
 * @returns {boolean}
 */
export function isSameNode(currentNode, desiredNode) {
    if (currentNode.nodeType !== desiredNode.nodeType)
        return false;
    if (currentNode.nodeType === Node.ELEMENT_NODE) {
        const currentElement = /** @type {Element} */ (currentNode);
        const desiredElement = /** @type {Element} */ (desiredNode);
        if (currentElement.tagName !== desiredElement.tagName)
            return false;
        const currentTacId = currentElement.getAttribute('data-tac-id');
        const desiredTacId = desiredElement.getAttribute('data-tac-id');
        if (currentTacId || desiredTacId)
            return currentTacId === desiredTacId;
        if (currentElement.id && desiredElement.id)
            return currentElement.id === desiredElement.id;
    }
    return true;
}

/**
 * Keeps live form-control state in sync with updated attributes.
 * @param {Element} currentElement
 * @param {Element} desiredElement
 */
function syncFormControlState(currentElement, desiredElement) {
    if (currentElement instanceof HTMLInputElement && desiredElement instanceof HTMLInputElement) {
        const nextValue = desiredElement.getAttribute('value') ?? desiredElement.value;
        if (currentElement.value !== nextValue)
            currentElement.value = nextValue;
        if (currentElement.checked !== desiredElement.checked)
            currentElement.checked = desiredElement.checked;
        return;
    }
    if (currentElement instanceof HTMLTextAreaElement && desiredElement instanceof HTMLTextAreaElement) {
        const nextValue = desiredElement.getAttribute('value') ?? desiredElement.value;
        if (currentElement.value !== nextValue)
            currentElement.value = nextValue;
        if (currentElement.textContent !== nextValue)
            currentElement.textContent = nextValue;
        return;
    }
    if (currentElement instanceof HTMLSelectElement && desiredElement instanceof HTMLSelectElement) {
        const nextValue = desiredElement.getAttribute('value') ?? desiredElement.value;
        if (currentElement.value !== nextValue)
            currentElement.value = nextValue;
    }
}

/**
 * The browser owns the live state of the control currently being edited. The
 * render that follows an input event can still contain the previous value, so
 * treating that markup as authoritative would erase the user's edit.
 * @param {Element} element
 */
function isActiveEditor(element) {
    if (document.activeElement !== element)
        return false;
    return isEditableElement(element);
}

/** @param {Element} element */
function isEditableElement(element) {
    return element instanceof HTMLInputElement
        || element instanceof HTMLTextAreaElement
        || element instanceof HTMLSelectElement
        || element.getAttribute('contenteditable') === 'true'
        || /** @type {HTMLElement} */ (element).isContentEditable === true;
}

/** @param {Element} current @param {Element} desired */
function canTransferEditorState(current, desired) {
    if (current.tagName !== desired.tagName || isEditableElement(current) !== isEditableElement(desired))
        return false;
    if (current instanceof HTMLInputElement && desired instanceof HTMLInputElement)
        return current.type === desired.type;
    if (current instanceof HTMLSelectElement && desired instanceof HTMLSelectElement)
        return current.multiple === desired.multiple;
    return true;
}

/**
 * @param {Element} element
 */
function captureActiveControlState(element) {
    if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) {
        return /** @type {ActiveControlState} */ ({
            kind: 'text',
            value: element.value,
            checked: element instanceof HTMLInputElement ? element.checked : undefined,
            selectionStart: element.selectionStart,
            selectionEnd: element.selectionEnd,
            selectionDirection: element.selectionDirection,
        });
    }
    if (element instanceof HTMLSelectElement) {
        return /** @type {ActiveControlState} */ ({
            kind: 'select',
            selectedOptions: Array.from(element.options)
                .flatMap((option, index) => option.selected ? [{
                    id: option.id,
                    value: option.value,
                    label: option.textContent ?? '',
                    index,
                }] : []),
        });
    }
    if (element.getAttribute('contenteditable') === 'true' || /** @type {HTMLElement} */ (element).isContentEditable === true) {
        const selection = document.getSelection();
        const anchorOffset = selection?.anchorNode && element.contains(selection.anchorNode)
            ? textOffsetWithin(element, selection.anchorNode, selection.anchorOffset)
            : null;
        const focusOffset = selection?.focusNode && element.contains(selection.focusNode)
            ? textOffsetWithin(element, selection.focusNode, selection.focusOffset)
            : null;
        return /** @type {ActiveControlState} */ ({
            kind: 'contenteditable',
            html: element.innerHTML,
            anchorOffset,
            focusOffset,
        });
    }
    return null;
}

/** @param {Element} root @param {Node} node @param {number} offset */
function textOffsetWithin(root, node, offset) {
    const prefix = document.createRange();
    prefix.selectNodeContents(root);
    prefix.setEnd(node, offset);
    return prefix.toString().length;
}

/** @param {Element} root @param {number} offset */
function textPositionAt(root, offset) {
    const walker = document.createTreeWalker(root, 4);
    let remaining = Math.max(0, offset);
    let node = walker.nextNode();
    while (node) {
        const length = node.textContent?.length ?? 0;
        if (remaining <= length)
            return { node, offset: remaining };
        remaining -= length;
        node = walker.nextNode();
    }
    return { node: root, offset: root.childNodes.length };
}

/**
 * @param {Element} element
 * @param {ActiveControlState | null} state
 */
function restoreActiveControlState(element, state) {
    if (!state)
        return;
    if (state.kind === 'text' && (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement)) {
        if (element.value !== state.value)
            element.value = state.value;
        if (element instanceof HTMLInputElement && state.checked !== undefined)
            element.checked = state.checked;
        if (typeof state.selectionStart === 'number' && typeof state.selectionEnd === 'number') {
            try {
                element.setSelectionRange(
                    state.selectionStart,
                    state.selectionEnd,
                    typeof state.selectionDirection === 'string' ? state.selectionDirection : undefined
                );
            }
            catch {
                // Some input types do not expose a selectable text range.
            }
        }
        return;
    }
    if (state.kind === 'select' && element instanceof HTMLSelectElement) {
        const options = Array.from(element.options);
        const selected = new Set();
        for (const previous of state.selectedOptions) {
            let match = previous.id
                ? options.find((option) => !selected.has(option) && option.id === previous.id)
                : undefined;
            match ??= options.find((option) => !selected.has(option)
                && option.value === previous.value
                && (option.textContent ?? '') === previous.label);
            match ??= options.find((option) => !selected.has(option) && option.value === previous.value);
            match ??= options[previous.index];
            if (match)
                selected.add(match);
        }
        for (const option of options)
            option.selected = selected.has(option);
        return;
    }
    if (state.kind === 'contenteditable'
        && (element.getAttribute('contenteditable') === 'true'
            || /** @type {HTMLElement} */ (element).isContentEditable === true)) {
        if (element.innerHTML !== state.html)
            element.innerHTML = state.html;
        if (state.anchorOffset !== null && state.focusOffset !== null) {
            const anchor = textPositionAt(element, state.anchorOffset);
            const focus = textPositionAt(element, state.focusOffset);
            const selection = document.getSelection();
            if (selection && typeof selection.setBaseAndExtent === 'function') {
                selection.setBaseAndExtent(anchor.node, anchor.offset, focus.node, focus.offset);
            }
            else if (selection) {
                const range = document.createRange();
                const start = state.anchorOffset <= state.focusOffset ? anchor : focus;
                const end = state.anchorOffset <= state.focusOffset ? focus : anchor;
                range.setStart(start.node, start.offset);
                range.setEnd(end.node, end.offset);
                selection.removeAllRanges();
                selection.addRange(range);
            }
        }
    }
}

/**
 * @param {Element} currentElement
 * @param {Element} desiredElement
 */
export function syncAttributes(currentElement, desiredElement) {
    const preserveLiveState = isActiveEditor(currentElement)
        && canTransferEditorState(currentElement, desiredElement);
    const liveState = preserveLiveState ? captureActiveControlState(currentElement) : null;
    for (const attribute of Array.from(currentElement.attributes)) {
        if (!attribute.name.startsWith('@') && !desiredElement.hasAttribute(attribute.name))
            currentElement.removeAttribute(attribute.name);
    }
    for (const attribute of Array.from(desiredElement.attributes)) {
        if (!attribute.name.startsWith('@') && currentElement.getAttribute(attribute.name) !== attribute.value)
            currentElement.setAttribute(attribute.name, attribute.value);
    }
    if (preserveLiveState)
        restoreActiveControlState(currentElement, liveState);
    else
        syncFormControlState(currentElement, desiredElement);
}

/** @param {Element} element */
function isCustomElement(element) {
    return element.localName.includes('-');
}

/**
 * Light DOM component libraries render their own shell around authored children.
 * Only slots owned by this host belong to its public child-content surface.
 * @param {Element} host
 * @returns {HTMLSlotElement[]}
 */
function ownedSlots(host) {
    return Array.from(host.querySelectorAll('slot')).filter((slot) => {
        let ancestor = slot.parentElement;
        while (ancestor && ancestor !== host) {
            if (isCustomElement(ancestor))
                return false;
            ancestor = ancestor.parentElement;
        }
        return ancestor === host;
    });
}

/**
 * Reconciles authored children inside a Light DOM component's slots without
 * touching the component-generated wrapper, controls, or event listeners.
 * @param {Element} currentElement
 * @param {Element} desiredElement
 * @param {MorphOptions} options
 */
function morphLightDomSlots(currentElement, desiredElement, options) {
    const slots = ownedSlots(currentElement);
    if (slots.length === 0)
        return false;
    /** @type {Map<string, Node[]>} */
    const desiredBySlot = new Map();
    for (const child of Array.from(desiredElement.childNodes)) {
        const slotName = child.nodeType === Node.ELEMENT_NODE
            ? /** @type {Element} */ (child).getAttribute('slot') ?? ''
            : '';
        const children = desiredBySlot.get(slotName) ?? [];
        children.push(child);
        desiredBySlot.set(slotName, children);
    }
    for (const slot of slots) {
        const desiredChildren = desiredBySlot.get(slot.name);
        if (!desiredChildren)
            continue;
        const fragment = document.createDocumentFragment();
        for (const child of desiredChildren)
            fragment.appendChild(child);
        morphChildren(slot, fragment, options);
    }
    return true;
}

/**
 * @param {Element | DocumentFragment} parent
 * @param {DocumentFragment} desired
 * @param {MorphOptions} [options]
 */
export function morphChildren(parent, desired, options = {}) {
    const currentChildren = Array.from(parent.childNodes);
    const desiredChildren = Array.from(desired.childNodes);
    const longestChildList = Math.max(currentChildren.length, desiredChildren.length);
    for (let index = 0; index < longestChildList; index++) {
        const currentChild = currentChildren[index];
        const desiredChild = desiredChildren[index];
        if (!currentChild && desiredChild) {
            parent.appendChild(desiredChild.cloneNode(true));
            continue;
        }
        if (currentChild && !desiredChild) {
            parent.removeChild(currentChild);
            continue;
        }
        if (!currentChild || !desiredChild)
            continue;
        if (!isSameNode(currentChild, desiredChild)) {
            parent.replaceChild(desiredChild.cloneNode(true), currentChild);
            continue;
        }
        if (currentChild.nodeType === Node.TEXT_NODE) {
            if (currentChild.textContent !== desiredChild.textContent)
                currentChild.textContent = desiredChild.textContent;
            continue;
        }
        if (currentChild.nodeType === Node.ELEMENT_NODE) {
            const currentElement = /** @type {Element} */ (currentChild);
            if (options.preserveElement?.(currentElement))
                continue;
            const desiredElement = /** @type {Element} */ (desiredChild);
            syncAttributes(currentElement, desiredElement);
            if (isActiveEditor(currentElement))
                continue;
            if (isCustomElement(currentElement)) {
                if (morphLightDomSlots(currentElement, desiredElement, options))
                    continue;
                // Slotless custom element (e.g. a light-DOM field/select that
                // renders its own controls with no authored child surface): it
                // owns its rendered subtree. Only reconcile when the authored
                // template actually provides children; otherwise leave the
                // component-generated DOM intact.
                if (desiredElement.childNodes.length === 0)
                    continue;
            }
            const desiredChildFragment = document.createDocumentFragment();
            while (desiredChild.firstChild)
                desiredChildFragment.appendChild(desiredChild.firstChild);
            morphChildren(currentElement, desiredChildFragment, options);
        }
    }
    while (parent.childNodes.length > desiredChildren.length) {
        parent.removeChild(/** @type {ChildNode} */ (parent.lastChild));
    }
}

/**
 * @param {string[]} input
 * @returns {ParamValue[]}
 */
export function parseParams(input) {
    return input.map(value => {
        const numberValue = Number(value);
        if (!Number.isNaN(numberValue))
            return numberValue;
        if (value === 'true')
            return true;
        if (value === 'false')
            return false;
        if (value === 'null')
            return null;
        if (value === 'undefined')
            return undefined;
        return value;
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
    let bestRoute = '';
    let bestRouteLength = -1;
    for (const [routeKey] of routes) {
        const routeSegments = routeKey.split('/').slice(1);
        if (routeSegments.length > segments.length)
            continue;
        const slugMap = routes.get(routeKey) ?? {};
        let matchesRoute = true;
        for (let index = 0; index < routeSegments.length; index++) {
            if (!slugMap[routeSegments[index]] && routeSegments[index] !== segments[index]) {
                matchesRoute = false;
                break;
            }
        }
        if (matchesRoute && routeSegments.length > bestRouteLength) {
            bestRoute = routeKey;
            bestRouteLength = routeSegments.length;
        }
    }
    if (!bestRoute)
        throw new Error(`Route ${pathname} not found`);
    const slugMap = routes.get(bestRoute) ?? {};
    for (const [key, segmentIndex] of Object.entries(slugMap)) {
        slugs[key.replace(':', '')] = segments[segmentIndex];
    }
    return bestRoute;
}
