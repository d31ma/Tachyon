// @ts-check
import { accessSync, constants, readFileSync } from 'fs';
import { access } from 'fs/promises';
import path from 'path';
import { pathToFileURL } from 'url';
import {
    HTTP_METHODS,
    providerForExtension,
    providerForLanguage,
    providerForShebang,
    registerLanguageProvider,
    registeredLanguages,
} from './language-providers.js';
import { resolveInterpreter } from '../../shared/toolchain-config.js';

/**
 * @typedef {string} YonHandlerLanguage A known-language provider id ('rust',
 *   'python', …) or 'executable' for the universal (any-language) path.
 *
 * @typedef {object} HandlerAdapterMatch
 * @property {YonHandlerLanguage} language
 * @property {string[]} command
 * @property {Set<string>} methods
 */

/** Language label for handlers run directly as executables (any language). */
const EXECUTABLE_LANGUAGE = 'executable';

/**
 * @param {string} commandPath
 * @returns {string}
 */
function basename(commandPath) {
    return commandPath.replaceAll('\\', '/').split('/').pop()?.toLowerCase() ?? commandPath.toLowerCase();
}

/** @param {string} filePath */
function isExecutableFile(filePath) {
    try {
        accessSync(filePath, constants.X_OK);
        return true;
    }
    catch {
        return false;
    }
}

/**
 * Methods for generic executable handlers come from an adjacent
 * OPTIONS.schema.json (the same sidecar convention Tac workers use) — the
 * framework cannot parse an arbitrary language's source for them.
 * @param {string} handler
 * @returns {Set<string>}
 */
function methodsFromOptionsSchema(handler) {
    /** @type {Set<string>} */
    const methods = new Set();
    try {
        const schema = JSON.parse(readFileSync(path.join(path.dirname(handler), 'OPTIONS.schema.json'), 'utf8'));
        for (const key of Object.keys(schema ?? {})) {
            if (HTTP_METHODS.has(key.toUpperCase()))
                methods.add(key.toUpperCase());
        }
    }
    catch {
        // Missing or malformed schema: no methods, caller rejects the handler.
    }
    return methods;
}

export default class HandlerAdapter {
    /** @type {Set<string>} Provider files already loaded this process. */
    static #loadedProviderFiles = new Set();

    /**
     * The languages Yon ships an ergonomic adapter for — NOT the set of
     * languages it "supports". Every language is supported: any executable
     * `yon.<ext>` that speaks the stdin/stdout protocol is a valid route.
     * These providers merely spare the developer the protocol glue for the
     * common languages. Exposed for tooling/diagnostics, not gatekeeping.
     * @returns {ReadonlyArray<string>}
     */
    static get knownLanguages() {
        return Object.freeze(registeredLanguages());
    }

    /**
     * Register a language provider programmatically. Apps normally export
     * providers from `server/yon.providers.js` instead.
     * @param {import('./language-providers.js').YonLanguageProvider} provider
     */
    static registerProvider(provider) {
        registerLanguageProvider(provider);
    }

    /**
     * Load app-defined providers from `<root>/server/yon.providers.js`
     * (default export: an array of providers). Loaded once per process.
     * @param {string} [root]
     */
    static async loadUserProviders(root = process.cwd()) {
        const file = path.join(root, 'server', 'yon.providers.js');
        if (HandlerAdapter.#loadedProviderFiles.has(file))
            return;
        HandlerAdapter.#loadedProviderFiles.add(file);
        try {
            await access(file);
        }
        catch {
            return;
        }
        const module = await import(pathToFileURL(file).href);
        const providers = Array.isArray(module.default) ? module.default : [];
        for (const provider of providers)
            registerLanguageProvider(provider);
    }

    /**
     * @param {string} handler
     * @param {string[]} shebangTokens
     * @returns {HandlerAdapterMatch | null}
     */
    static resolve(handler, shebangTokens) {
        let source = '';
        try {
            source = readFileSync(handler, 'utf8');
        }
        catch {
            return null;
        }
        // Convenience path: a language Yon ships an adapter for, whose source
        // follows the `class Handler` convention, runs through that adapter —
        // the developer writes only handler methods, no stdin/stdout glue.
        const language = HandlerAdapter.detectLanguage(handler, shebangTokens, source);
        const provider = language ? providerForLanguage(language) : null;
        if (provider && provider.hasHandlerClass(source)) {
            const methods = HandlerAdapter.detectMethods(source, /** @type {string} */ (language));
            if (methods.size > 0) {
                return {
                    language: /** @type {YonHandlerLanguage} */ (language),
                    command: provider.command(handler),
                    methods,
                };
            }
        }
        // Universal path: ANY language, run by extension — no shebang. The
        // extension names the language and `.tachyonrc` interpreters map it
        // to a run command (`.go` → `go run`, seeded with common defaults,
        // fully overridable). A handler that is itself executable (a prebuilt
        // binary, or a script with its own shebang) is also accepted. Either
        // way the handler speaks the stdin/stdout JSON protocol and declares
        // its HTTP methods in the adjacent OPTIONS.schema.json, since the
        // framework does not parse arbitrary source. The only limit is the
        // developer's toolchain; there is no "supported" list.
        const interpreter = resolveInterpreter(path.extname(handler));
        if (interpreter || isExecutableFile(handler)) {
            const methods = methodsFromOptionsSchema(handler);
            if (methods.size > 0) {
                return {
                    language: /** @type {YonHandlerLanguage} */ (language ?? EXECUTABLE_LANGUAGE),
                    command: interpreter ? [...interpreter, handler] : [handler],
                    methods,
                };
            }
        }
        return null;
    }

    /**
     * @param {YonHandlerLanguage} language
     * @param {string} handler
     * @returns {string[]}
     */
    static commandFor(language, handler) {
        if (language === EXECUTABLE_LANGUAGE)
            return [handler];
        const provider = providerForLanguage(language);
        if (!provider)
            throw new Error(`No Yon language provider registered for '${language}'`);
        return provider.command(handler);
    }

    /**
     * @param {string} handler
     * @param {string[]} shebangTokens
     * @param {string} source
     * @returns {YonHandlerLanguage | null}
     */
    static detectLanguage(handler, shebangTokens, source) {
        const extension = path.extname(handler).toLowerCase();
        const byExtension = extension ? providerForExtension(extension) : null;
        if (byExtension)
            return byExtension.language;

        const command = basename(shebangTokens[0] ?? '');
        const byShebang = providerForShebang(command);
        if (!byShebang)
            return null;
        // JS runtimes execute both dialects — sniff the source to keep the
        // historical TypeScript detection for extension-less handlers.
        if (byShebang.language === 'javascript') {
            return source.includes(':') && /\bexport\s+(?:async\s+)?function\s+handler\b/.test(source)
                ? 'typescript'
                : 'javascript';
        }
        return byShebang.language;
    }

    /**
     * @param {string} source
     * @param {YonHandlerLanguage} language
     * @returns {boolean}
     */
    static hasHandlerClass(source, language) {
        return providerForLanguage(language)?.hasHandlerClass(source) ?? false;
    }

    /**
     * @param {string} source
     * @param {YonHandlerLanguage} language
     * @returns {Set<string>}
     */
    static detectMethods(source, language) {
        /** @type {Set<string>} */
        const methods = new Set();
        for (const method of HTTP_METHODS) {
            if (HandlerAdapter.hasMethod(source, language, method))
                methods.add(method);
        }
        return methods;
    }

    /**
     * @param {string} source
     * @param {YonHandlerLanguage} language
     * @param {string} method
     * @returns {boolean}
     */
    static hasMethod(source, language, method) {
        return providerForLanguage(language)?.hasMethod(source, method) ?? false;
    }
}
