// @ts-check
import {
    HTML_ELEMENT_SET,
    HTML_VOID_ELEMENT_SET,
    NATIVE_UI_ELEMENT_SET,
    TAC_CONTROL_ELEMENT_SET,
} from '../compiler/html-tags.js';
import {
    collectNativeUIScopedStyles,
    nativeUIAdapterMap,
    nativeUIAdaptersModuleSource,
    scopeNativeUIBoundaryHTML,
} from '../compiler/native-ui/adapters.js';

const NON_VISUAL_ELEMENTS = new Set(['script', 'style', 'template', 'noscript']);

/** @param {string} source */
function decodeEntities(source) {
    return source.replace(/&(?:#(\d+)|#x([0-9a-f]+)|([a-z]+));/gi, (match, decimal, hexadecimal, named) => {
        if (decimal) return String.fromCodePoint(Number.parseInt(decimal, 10));
        if (hexadecimal) return String.fromCodePoint(Number.parseInt(hexadecimal, 16));
        switch (String(named).toLowerCase()) {
            case 'amp': return '&';
            case 'lt': return '<';
            case 'gt': return '>';
            case 'quot': return '"';
            case 'apos': return "'";
            case 'nbsp': return '\u00a0';
            default: return match;
        }
    });
}

/** @param {string} source */
function parseStyle(source) {
    /** @type {Record<string, string>} */
    const style = {};
    for (const declaration of source.split(';')) {
        const separator = declaration.indexOf(':');
        if (separator < 1) continue;
        const name = declaration.slice(0, separator).trim().toLowerCase();
        const value = declaration.slice(separator + 1).trim();
        if (name && value) style[name] = decodeEntities(value);
    }
    return style;
}

/** @param {string} source */
function parseAttributes(source) {
    /** @type {Record<string, string>} */
    const attributes = {};
    const pattern = /([^\s=/>]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?/g;
    for (const match of source.matchAll(pattern)) {
        const name = match[1].toLowerCase();
        attributes[name] = decodeEntities(match[2] ?? match[3] ?? match[4] ?? '');
    }
    return attributes;
}

/** @param {string} html @param {number} start */
function findTagEnd(html, start) {
    let quote = '';
    for (let index = start; index < html.length; index += 1) {
        const character = html[index];
        if (quote) {
            if (character === quote) quote = '';
            continue;
        }
        if (character === '"' || character === "'") {
            quote = character;
            continue;
        }
        if (character === '>') return index;
    }
    throw new Error('Native UI HTML contains an unterminated start tag.');
}

/** @param {string} html @param {number} cursor @param {string} rootTag */
function findElementEnd(html, cursor, rootTag) {
    let depth = 1;
    const normalizedHTML = html.toLowerCase();
    while (cursor < html.length) {
        const opening = html.indexOf('<', cursor);
        if (opening < 0) break;
        if (html.startsWith('<!--', opening)) {
            const commentEnd = html.indexOf('-->', opening + 4);
            if (commentEnd < 0) throw new Error('Native UI HTML contains an unterminated comment.');
            cursor = commentEnd + 3;
            continue;
        }
        const closing = html.startsWith('</', opening);
        const end = findTagEnd(html, opening + 1);
        const rawTag = html.slice(opening + (closing ? 2 : 1), end).trim();
        const selfClosing = rawTag.endsWith('/');
        const tag = rawTag.replace(/\/$/, '').trim().split(/\s/, 1)[0].toLowerCase();
        if (!closing && (tag === 'script' || tag === 'style')) {
            const closeTag = `</${tag}>`;
            const closingIndex = normalizedHTML.indexOf(closeTag, end + 1);
            if (closingIndex < 0)
                throw new Error(`Native UI HTML contains an unterminated <${tag}> tag.`);
            cursor = closingIndex + closeTag.length;
            continue;
        }
        if (tag === rootTag) {
            if (closing) depth -= 1;
            else if (!selfClosing && !HTML_VOID_ELEMENT_SET.has(tag)) depth += 1;
            if (depth === 0) return end + 1;
        }
        cursor = end + 1;
    }
    throw new Error(`Native UI HTML is missing closing tag </${rootTag}>.`);
}

/** @param {any[]} children @param {string} raw */
function appendText(children, raw) {
    if (!raw || /^\s+$/.test(raw)) return;
    const value = decodeEntities(raw);
    const previous = children.at(-1);
    if (previous?.kind === 'text') previous.value += value;
    else children.push({ kind: 'text', value });
}

/**
 * Parses the deterministic, well-formed HTML emitted by a compiled Tac render
 * closure. It intentionally is not a browser error-recovery parser: malformed
 * output fails closed instead of producing a different native hierarchy.
 *
 * @param {string} html
 * @param {{ route?: string, adapters?: string[] | Record<string, string> }} [options]
 */
export function parseNativeUIFragment(html, options = {}) {
    const route = options.route ?? '/';
    const adapters = nativeUIAdapterMap(options.adapters);
    const scopedStyles = collectNativeUIScopedStyles(html);
    const fragment = { kind: 'fragment', children: [] };
    /** @type {Array<{ tag: string, node: any, ignored: boolean }>} */
    const stack = [{ tag: '#fragment', node: fragment, ignored: false }];
    let cursor = 0;

    while (cursor < html.length) {
        const opening = html.indexOf('<', cursor);
        const current = stack.at(-1);
        if (!current) throw new Error('Native UI parser stack is unexpectedly empty.');
        if (opening < 0) {
            if (!current.ignored) appendText(current.node.children, html.slice(cursor));
            break;
        }
        if (opening > cursor && !current.ignored)
            appendText(current.node.children, html.slice(cursor, opening));

        if (html.startsWith('<!--', opening)) {
            const end = html.indexOf('-->', opening + 4);
            if (end < 0) throw new Error('Native UI HTML contains an unterminated comment.');
            cursor = end + 3;
            continue;
        }
        const closing = html.startsWith('</', opening);
        const end = findTagEnd(html, opening + 1);
        const rawTag = html.slice(opening + (closing ? 2 : 1), end).trim();
        cursor = end + 1;
        if (!rawTag || rawTag.startsWith('!') || rawTag.startsWith('?'))
            continue;

        if (closing) {
            const tag = rawTag.split(/\s/, 1)[0].toLowerCase();
            const current = stack.at(-1);
            if (!current || current.tag !== tag)
                throw new Error(`Native UI HTML contains mismatched closing tag </${tag}>; expected </${current?.tag ?? 'none'}>.`);
            stack.pop();
            continue;
        }

        const selfClosing = rawTag.endsWith('/');
        const content = selfClosing ? rawTag.slice(0, -1).trim() : rawTag;
        const nameEnd = content.search(/\s/);
        const tag = (nameEnd < 0 ? content : content.slice(0, nameEnd)).toLowerCase();
        const attributeSource = nameEnd < 0 ? '' : content.slice(nameEnd + 1);
        if (TAC_CONTROL_ELEMENT_SET.has(tag))
            throw new Error(`Native UI route '${route}' contains unresolved Tac <${tag}> control flow.`);
        const customElement = tag.includes('-');
        const adapterTarget = customElement ? adapters[tag] : null;
        if (!customElement && !HTML_ELEMENT_SET.has(tag))
            throw new Error(`Unknown HTML element <${tag}> cannot be rendered natively in route '${route}'.`);
        const rawAttributes = parseAttributes(attributeSource);
        const hasBrowserBehavior = Object.keys(rawAttributes).some((name) => name.startsWith('w-'));
        const requiresWebView = (customElement && !adapterTarget)
            || (!customElement && !NATIVE_UI_ELEMENT_SET.has(tag))
            || (!adapterTarget && hasBrowserBehavior);
        if (requiresWebView) {
            const id = rawAttributes['data-tac-id'] || rawAttributes.id || null;
            const key = rawAttributes.key || rawAttributes['data-tac-key'] || null;
            const style = parseStyle(rawAttributes.style || '');
            /** @type {Record<string, string>} */
            const attributes = {};
            /** @type {Record<string, string>} */
            const events = {};
            for (const [name, value] of Object.entries(rawAttributes)) {
                if (name === 'id' || name === 'data-tac-id' || name === 'key' || name === 'data-tac-key' || name === 'style') continue;
                if (name.startsWith('data-tac-on-')) {
                    const eventName = name.slice('data-tac-on-'.length).replaceAll('__', ':');
                    if (!id) throw new Error(`WebView boundary event '${eventName}' on <${tag}> requires a stable element id.`);
                    events[eventName] = id;
                }
                else attributes[name] = value;
            }
            const boundary = selfClosing || HTML_VOID_ELEMENT_SET.has(tag) ? cursor : findElementEnd(html, cursor, tag);
            const parent = stack.at(-1);
            if (!parent) throw new Error('Native UI parser stack is unexpectedly empty.');
            const scopes = stack.flatMap((ancestor) => {
                const scope = ancestor.node?.attributes?.['data-tac-scope'];
                return scope ? [scope] : [];
            });
            parent.node.children.push({
                kind: 'webview', tag, id, key, adapter: null, attributes, style, events,
                html: scopeNativeUIBoundaryHTML(html.slice(opening, boundary), scopes, scopedStyles), children: [],
            });
            cursor = boundary;
            continue;
        }
        const ignored = stack.at(-1)?.ignored || NON_VISUAL_ELEMENTS.has(tag);
        if (ignored) {
            if (!selfClosing && !HTML_VOID_ELEMENT_SET.has(tag))
                stack.push({ tag, node: { children: [] }, ignored: true });
            continue;
        }

        const id = rawAttributes['data-tac-id'] || rawAttributes.id || null;
        const key = rawAttributes.key || rawAttributes['data-tac-key'] || null;
        const style = parseStyle(rawAttributes.style || '');
        /** @type {Record<string, string>} */
        const attributes = {};
        /** @type {Record<string, string>} */
        const events = {};
        for (const [name, value] of Object.entries(rawAttributes)) {
            if (name === 'id' || name === 'data-tac-id' || name === 'key' || name === 'data-tac-key' || name === 'style') continue;
            if (name.startsWith('data-tac-on-')) {
                const eventName = name.slice('data-tac-on-'.length).replaceAll('__', ':');
                if (!id) throw new Error(`Native UI event '${eventName}' on <${tag}> requires a stable element id.`);
                events[eventName] = id;
            }
            else attributes[name] = value;
        }
        const node = {
            kind: 'element', tag: adapterTarget ?? tag, id, key, adapter: customElement ? tag : null,
            attributes, style, events, children: [],
        };
        const parent = stack.at(-1);
        if (!parent) throw new Error('Native UI parser stack is unexpectedly empty.');
        parent.node.children.push(node);
        if (!selfClosing && !HTML_VOID_ELEMENT_SET.has(tag))
            stack.push({ tag, node, ignored: false });
    }

    if (stack.length !== 1)
        throw new Error(`Native UI HTML is missing closing tag </${stack.at(-1)?.tag}>.`);
    return fragment.children.length === 1 ? fragment.children[0] : fragment;
}

/** @param {any} root */
function collectEvents(root) {
    /** @type {Map<string, Set<string>>} */
    const events = new Map();
    /** @param {any} node */
    const visit = (node) => {
        if (!node || typeof node !== 'object') return;
        if (node.kind === 'element' && node.id) {
            const names = new Set(Object.keys(node.events ?? {}));
            if (names.size) events.set(node.id, names);
        }
        for (const child of node.children ?? []) visit(child);
    };
    visit(root);
    return events;
}

export default class NativeUIRuntime {
    /**
     * @param {(elementId?: string, event?: any, componentId?: string) => Promise<string> | string} renderClosure
     * @param {{ route: string, adapters?: string[] | Record<string, string> }} options
     */
    constructor(renderClosure, options) {
        this.renderClosure = renderClosure;
        this.route = options.route;
        this.adapters = options.adapters ?? [];
        this.snapshot = null;
        this.events = new Map();
    }

    /** @param {string | undefined} [elementId] @param {any} [event] */
    async render(elementId = undefined, event = undefined) {
        const html = await this.renderClosure(elementId, event);
        const root = parseNativeUIFragment(html, { route: this.route, adapters: this.adapters });
        this.events = collectEvents(root);
        this.snapshot = { schemaVersion: 1, route: this.route, root };
        return this.snapshot;
    }

    /** @param {{ elementId: string, type: string, value?: unknown, checked?: boolean, detail?: unknown }} nativeEvent */
    async dispatch(nativeEvent) {
        if (!this.snapshot) await this.render();
        const eventNames = this.events.get(nativeEvent.elementId);
        if (!eventNames?.has(nativeEvent.type))
            throw new Error(`Native UI element '${nativeEvent.elementId}' does not handle '${nativeEvent.type}'.`);
        const target = { value: nativeEvent.value, checked: nativeEvent.checked };
        const detail = nativeEvent.detail ?? target;
        const event = {
            type: nativeEvent.type,
            value: nativeEvent.value,
            checked: nativeEvent.checked,
            detail,
            target: { ...target },
            currentTarget: { ...target },
        };
        return this.render(nativeEvent.elementId, event);
    }
}

/**
 * Serializes the DOM-free native renderer into a virtual ES module. This is
 * deliberately generated from the live functions so source and standalone
 * binary builds cannot drift apart.
 */
export function nativeUIRuntimeModuleSource() {
    return `${nativeUIAdaptersModuleSource()}
const HTML_ELEMENT_SET = new Set(${JSON.stringify([...HTML_ELEMENT_SET])});
const HTML_VOID_ELEMENT_SET = new Set(${JSON.stringify([...HTML_VOID_ELEMENT_SET])});
const NATIVE_UI_ELEMENT_SET = new Set(${JSON.stringify([...NATIVE_UI_ELEMENT_SET])});
const TAC_CONTROL_ELEMENT_SET = new Set(${JSON.stringify([...TAC_CONTROL_ELEMENT_SET])});
const NON_VISUAL_ELEMENTS = new Set(${JSON.stringify([...NON_VISUAL_ELEMENTS])});
${decodeEntities.toString()}
${parseStyle.toString()}
${parseAttributes.toString()}
${findTagEnd.toString()}
${findElementEnd.toString()}
${appendText.toString()}
export ${parseNativeUIFragment.toString()}
${collectEvents.toString()}
export default ${NativeUIRuntime.toString()};
`;
}
