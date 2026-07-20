// @ts-check
import {
    HTML_ELEMENT_SET,
    HTML_VOID_ELEMENT_SET,
    NATIVE_UI_ELEMENT_SET,
    TAC_CONTROL_ELEMENT_SET,
} from '../html-tags.js';
import { collectNativeUIScopedStyles, nativeUIAdapterMap, scopeNativeUIBoundaryHTML } from './adapters.js';

/** @typedef {{ kind: 'text', value: string }} NativeUITextNode */
/** @typedef {{ kind: 'fragment', children: NativeUINode[] }} NativeUIFragmentNode */
/**
 * @typedef {{
 *   kind: 'webview', tag: string, id: string | null, key: string | null,
 *   adapter: null, attributes: Record<string, string>, style: Record<string, string>,
 *   events: Record<string, string>, html: string, children: [],
 * }} NativeUIWebViewNode
 */
/**
 * @typedef {{
 *   kind: 'element',
 *   tag: string,
 *   id: string | null,
 *   key: string | null,
 *   adapter: string | null,
 *   attributes: Record<string, string>,
 *   style: Record<string, string>,
 *   events: Record<string, string>,
 *   children: NativeUINode[],
 * }} NativeUIElementNode
 */
/** @typedef {NativeUITextNode | NativeUIFragmentNode | NativeUIElementNode | NativeUIWebViewNode} NativeUINode */
/**
 * @typedef {{
 *   schemaVersion: 1,
 *   route: string,
 *   title: string,
 *   root: NativeUINode,
 * }} NativeUIDocument
 */

const NON_VISUAL_ELEMENTS = new Set(['script', 'style', 'template', 'noscript']);

/** @param {string} source */
function parseStyle(source) {
    /** @type {Record<string, string>} */
    const declarations = {};
    for (const part of source.split(';')) {
        const separator = part.indexOf(':');
        if (separator < 1)
            continue;
        const property = part.slice(0, separator).trim().toLowerCase();
        const value = part.slice(separator + 1).trim();
        if (property && value)
            declarations[property] = value;
    }
    return declarations;
}

/** @param {NativeUINode[]} children @param {string} value */
function appendText(children, value) {
    if (!value || /^\s+$/.test(value))
        return;
    const previous = children.at(-1);
    if (previous?.kind === 'text')
        previous.value += value;
    else
        children.push({ kind: 'text', value });
}

/** @param {string} value */
function escapeHTML(value) {
    return value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
}

/** @param {string} value */
function escapeAttribute(value) {
    return escapeHTML(value).replaceAll('"', '&quot;');
}

/** @param {any} element */
function serializeStartTag(element) {
    const attributes = [...element.attributes]
        .map(([name, value]) => ` ${name}="${escapeAttribute(value)}"`)
        .join('');
    return `<${element.tagName.toLowerCase()}${attributes}>`;
}

export default class NativeUIDocumentCompiler {
    /**
     * @param {string} html
     * @param {{ route: string, adapters?: string[] | Record<string, string> }} options
     * @returns {Promise<NativeUIDocument>}
     */
    static async compile(html, options) {
        const adapters = nativeUIAdapterMap(options.adapters);
        const scopedStyles = collectNativeUIScopedStyles(html);
        /** @type {NativeUIFragmentNode & { ignored?: boolean }} */
        const body = { kind: 'fragment', children: [] };
        /** @type {Array<(NativeUIElementNode | NativeUIFragmentNode) & { ignored?: boolean }>} */
        const stack = [];
        let title = '';
        /** @type {{ node: NativeUIWebViewNode, parts: string[], depth: number, rawTextDepth: number, scopes: string[] } | null} */
        let capture = null;

        const rewriter = new HTMLRewriter()
            .on('title', {
                text(chunk) { title += chunk.text; },
            })
            .on('body', {
                element(element) {
                    stack.push(body);
                    element.onEndTag(() => { stack.pop(); });
                },
            })
            .on('body *', {
                element(element) {
                    const tag = element.tagName.toLowerCase();
                    if (capture) {
                        const active = capture;
                        active.parts.push(serializeStartTag(element));
                        if (!HTML_VOID_ELEMENT_SET.has(tag)) {
                            active.depth += 1;
                            if (tag === 'script' || tag === 'style') active.rawTextDepth += 1;
                            element.onEndTag(() => {
                                active.parts.push(`</${tag}>`);
                                if (tag === 'script' || tag === 'style') active.rawTextDepth -= 1;
                                active.depth -= 1;
                                if (active.depth === 0) active.node.html = scopeNativeUIBoundaryHTML(active.parts.join(''), active.scopes, scopedStyles);
                            });
                        }
                        return;
                    }
                    if (TAC_CONTROL_ELEMENT_SET.has(tag))
                        throw new Error(`Native UI route '${options.route}' contains unresolved Tac <${tag}> control flow.`);

                    const customElement = tag.includes('-');
                    const adapterTarget = customElement ? adapters[tag] : null;
                    if (!customElement && !HTML_ELEMENT_SET.has(tag))
                        throw new Error(`Unknown HTML element <${tag}> cannot be rendered natively in route '${options.route}'.`);
                    const hasBrowserBehavior = [...element.attributes]
                        .some(([name]) => name.toLowerCase().startsWith('w-'));
                    const requiresWebView = (customElement && !adapterTarget)
                        || (!customElement && !NATIVE_UI_ELEMENT_SET.has(tag))
                        || (!adapterTarget && hasBrowserBehavior);

                    if (requiresWebView) {
                        /** @type {Record<string, string>} */
                        const attributes = {};
                        /** @type {Record<string, string>} */
                        const events = {};
                        const eventNames = [];
                        let id = null;
                        let key = null;
                        /** @type {Record<string, string>} */
                        let style = {};
                        for (const [rawName, value] of element.attributes) {
                            const name = rawName.toLowerCase();
                            if (name === 'id') id = value;
                            else if (name === 'key' || name === 'data-tac-key') key = value;
                            else if (name === 'style') style = parseStyle(value);
                            else if (name.startsWith('data-tac-on-')) eventNames.push(name.slice('data-tac-on-'.length).replaceAll('__', ':'));
                            else attributes[name] = value;
                        }
                        const dispatchId = attributes['data-tac-id'] || id;
                        delete attributes['data-tac-id'];
                        for (const eventName of eventNames) {
                            if (!dispatchId) throw new Error(`WebView boundary event '${eventName}' on <${tag}> requires a stable element id.`);
                            events[eventName] = dispatchId;
                        }
                        /** @type {NativeUIWebViewNode} */
                        const node = {
                            kind: 'webview', tag, id: dispatchId, key, adapter: null,
                            attributes, style, events, html: '', children: [],
                        };
                        const parent = stack.at(-1);
                        if (!parent) throw new Error(`WebView boundary <${tag}> was emitted outside <body>.`);
                        if (!parent.ignored) parent.children.push(node);
                        const scopes = stack.flatMap((ancestor) => {
                            if (ancestor.kind !== 'element') return [];
                            const scope = ancestor.attributes['data-tac-scope'];
                            return scope ? [scope] : [];
                        });
                        const parts = [serializeStartTag(element)];
                        if (HTML_VOID_ELEMENT_SET.has(tag)) node.html = scopeNativeUIBoundaryHTML(parts.join(''), scopes, scopedStyles);
                        else {
                            const active = { node, parts, depth: 1, rawTextDepth: 0, scopes };
                            capture = active;
                            element.onEndTag(() => {
                                active.parts.push(`</${tag}>`);
                                active.depth -= 1;
                                if (active.depth === 0) {
                                    active.node.html = scopeNativeUIBoundaryHTML(active.parts.join(''), active.scopes, scopedStyles);
                                    if (capture === active) capture = null;
                                }
                            });
                        }
                        return;
                    }

                    if (NON_VISUAL_ELEMENTS.has(tag)) {
                        if (!HTML_VOID_ELEMENT_SET.has(tag)) {
                            const ignored = /** @type {NativeUIFragmentNode & { ignored: boolean }} */ ({ kind: 'fragment', children: [], ignored: true });
                            stack.push(ignored);
                            element.onEndTag(() => { stack.pop(); });
                        }
                        return;
                    }

                    /** @type {Record<string, string>} */
                    const attributes = {};
                    /** @type {Record<string, string>} */
                    const events = {};
                    /** @type {string[]} */
                    const eventNames = [];
                    let id = null;
                    let key = null;
                    /** @type {Record<string, string>} */
                    let style = {};
                    for (const [rawName, value] of element.attributes) {
                        const name = rawName.toLowerCase();
                        if (name === 'id') id = value;
                        else if (name === 'key' || name === 'data-tac-key') key = value;
                        else if (name === 'style') style = parseStyle(value);
                        else if (name.startsWith('data-tac-on-')) {
                            const eventName = name.slice('data-tac-on-'.length).replaceAll('__', ':');
                            eventNames.push(eventName);
                        }
                        else attributes[name] = value;
                    }
                    const dispatchId = attributes['data-tac-id'] || id;
                    delete attributes['data-tac-id'];
                    for (const eventName of eventNames) {
                        if (!dispatchId)
                            throw new Error(`Native UI event '${eventName}' on <${tag}> requires a stable element id.`);
                        events[eventName] = dispatchId;
                    }

                    /** @type {NativeUIElementNode} */
                    const node = {
                        kind: 'element',
                        tag: adapterTarget ?? tag,
                        id: dispatchId,
                        key,
                        adapter: customElement ? tag : null,
                        attributes,
                        style,
                        events,
                        children: [],
                    };
                    const parent = stack.at(-1);
                    if (!parent)
                        throw new Error(`Native UI route '${options.route}' emitted <${tag}> outside <body>.`);
                    if (!parent.ignored)
                        parent.children.push(node);
                    if (!HTML_VOID_ELEMENT_SET.has(tag)) {
                        stack.push(node);
                        element.onEndTag(() => { stack.pop(); });
                    }
                },
                text(chunk) {
                    if (capture) {
                        // HTMLRewriter exposes serialized entity text here.
                        // Preserve it verbatim so a boundary is not encoded a
                        // second time before the WebView parses it.
                        capture.parts.push(chunk.text);
                        return;
                    }
                    const parent = stack.at(-1);
                    if (parent && !parent.ignored)
                        appendText(parent.children, chunk.text);
                },
            });

        await rewriter.transform(new Response(html)).text();
        const root = body.children.length === 1 ? body.children[0] : body;
        return {
            schemaVersion: 1,
            route: options.route,
            title: title.trim(),
            root,
        };
    }
}
