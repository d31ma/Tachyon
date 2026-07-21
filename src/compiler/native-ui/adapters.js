// @ts-check
import { NATIVE_UI_ELEMENT_SET } from '../html-tags.js';

const CUSTOM_ELEMENT_PATTERN = /^[a-z][.0-9_a-z]*-[.0-9_a-z-]*$/;
const NON_VISUAL_ELEMENTS = new Set(['script', 'style', 'template', 'noscript']);

/** @typedef {string[] | Record<string, string>} NativeUIAdapters */

/** @param {string} tag */
function assertCustomElementTag(tag) {
    if (!CUSTOM_ELEMENT_PATTERN.test(tag)) {
        throw new Error(`Invalid native UI adapter tag '${tag}'. Adapter tags must be valid hyphenated custom-element names.`);
    }
}

/**
 * Validates and deterministically orders the public nativeUIAdapters value.
 * Arrays retain the original generic-adapter behavior; object values lower a
 * custom element to a schema element shared by every native host.
 *
 * @param {unknown} value
 * @returns {NativeUIAdapters}
 */
export function normalizeNativeUIAdapters(value) {
    if (value === undefined)
        return [];
    if (Array.isArray(value)) {
        const tags = value.map((entry) => String(entry).trim().toLowerCase());
        for (const tag of tags)
            assertCustomElementTag(tag);
        return [...new Set(tags)].sort();
    }
    if (!value || typeof value !== 'object') {
        throw new Error('tac.config.js nativeUIAdapters must be an array of custom-element tag names or an object mapping custom elements to native HTML semantics.');
    }

    /** @type {Record<string, string>} */
    const mappings = {};
    for (const [rawAdapter, rawTarget] of Object.entries(value)) {
        const adapter = rawAdapter.trim().toLowerCase();
        const target = typeof rawTarget === 'string' ? rawTarget.trim().toLowerCase() : '';
        assertCustomElementTag(adapter);
        if (!NATIVE_UI_ELEMENT_SET.has(target) || NON_VISUAL_ELEMENTS.has(target)) {
            throw new Error(`Native UI adapter '${adapter}' maps to <${target || String(rawTarget)}>, which has no visual schema-v1 native UI mapping.`);
        }
        mappings[adapter] = target;
    }
    return Object.fromEntries(Object.entries(mappings).sort(([left], [right]) => left.localeCompare(right)));
}

/**
 * @param {unknown} value
 * @returns {Record<string, string>}
 */
export function nativeUIAdapterMap(value) {
    const adapters = normalizeNativeUIAdapters(value);
    if (!Array.isArray(adapters))
        return adapters;
    return Object.fromEntries(adapters.map((tag) => [tag, tag]));
}

/**
 * A boundary is loaded from the bundled asset root rather than an HTTP origin.
 * Keep authored root-relative asset URLs rooted at that bundle on every host.
 *
 * @param {string} html
 */
export function normalizeNativeUIBoundaryHTML(html) {
    return html.replace(/\b(src|href|poster)=(['"])\//gi, '$1=$2');
}

/** @param {string} html */
export function collectNativeUIScopedStyles(html) {
    /** @type {Map<string, string>} */
    const styles = new Map();
    const pattern = /<[^>]*\bdata-tac-scope=(['"])(.*?)\1[^>]*>\s*<style>([\s\S]*?)<\/style>/gi;
    for (const match of html.matchAll(pattern))
        styles.set(match[2], `${styles.get(match[2]) ?? ''}${match[3]}`);
    return styles;
}

/** @param {string} html @param {string[]} scopes @param {Map<string, string>} [styles] */
export function scopeNativeUIBoundaryHTML(html, scopes, styles = new Map()) {
    let scoped = normalizeNativeUIBoundaryHTML(html);
    for (const value of [...scopes].reverse()) {
        const escaped = value.replaceAll('&', '&amp;').replaceAll('"', '&quot;');
        scoped = `<div data-tac-scope="${escaped}">${scoped}</div>`;
    }
    const css = scopes.map((scope) => styles.get(scope) ?? '').filter(Boolean).join('\n');
    return css ? `<style>${css}</style>${scoped}` : scoped;
}

/**
 * Returns this module as self-contained source for the compiled standalone
 * CLI. Bun's executable compiler does not preserve arbitrary source paths, so
 * native controller builds load this source through an in-memory plugin.
 */
export function nativeUIAdaptersModuleSource() {
    return `const CUSTOM_ELEMENT_PATTERN = ${CUSTOM_ELEMENT_PATTERN};
${assertCustomElementTag.toString()}
export ${normalizeNativeUIAdapters.toString()}
export ${nativeUIAdapterMap.toString()}
export ${normalizeNativeUIBoundaryHTML.toString()}
export ${collectNativeUIScopedStyles.toString()}
export ${scopeNativeUIBoundaryHTML.toString()}
`;
}
