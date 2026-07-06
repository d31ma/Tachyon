// @ts-check
//
// Yon language-provider registry.
//
// A Yon route handler is, at bottom, anything that can speak the adapter
// protocol: read one JSON request envelope from stdin (`{ method, headers,
// paths, query, body }`), write the response body (string or JSON) to
// stdout, report errors on stderr with a non-zero exit. Language "support"
// is nothing more than a provider that knows how to detect a handler file
// and produce the argv that speaks that protocol for it.
//
// Built-in providers cover the languages Yon has always shipped. Apps add
// their own from `server/yon.providers.js` (default-export an array of
// providers), and any language without a provider still works through the
// generic executable fallback in handler-adapter.js: an executable
// `yon.<ext>` file plus an adjacent `OPTIONS.schema.json` declaring its
// HTTP methods is a valid route — Yon never needs to know the language.

import path from 'path';

/**
 * @typedef {object} YonLanguageProvider
 * @property {string} language Unique id ('rust', 'go', …). Registering an
 *   existing id replaces the built-in — that is how an app overrides, say,
 *   the python command with its own virtualenv interpreter.
 * @property {string[]} extensions Handler file extensions, with dot ('.rs').
 * @property {string[]} [shebangs] Shebang command basenames that map to this
 *   provider when the handler file has no recognised extension. Entries
 *   ending in '*' match as prefixes ('python*' covers python3.12).
 * @property {(handler: string) => string[]} command Argv that serves one
 *   request for this handler over the stdin/stdout protocol.
 * @property {(source: string) => boolean} hasHandlerClass Whether the source
 *   declares the `Handler` class convention.
 * @property {(source: string, method: string) => boolean} hasMethod Whether
 *   the source implements the given HTTP method on the Handler class.
 */

const ADAPTER_DIR = path.join(import.meta.dir, 'adapters');

/** HTTP methods that map to static method names on the Handler class. */
export const HTTP_METHODS = new Set(['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS']);

/** @returns {string} */
function phpExecutable() {
    if (process.platform !== 'win32')
        return 'php';
    const localAppData = process.env.LOCALAPPDATA;
    const candidate = localAppData
        ? path.join(localAppData, 'Microsoft', 'WinGet', 'Packages', 'PHP.PHP.8.4_Microsoft.Winget.Source_8wekyb3d8bbwe', 'php.exe')
        : '';
    return candidate || 'php';
}

/** @param {string} source */
const hasClassHandler = (source) => /\bclass\s+Handler\b/.test(source);

/** @param {string} handler @returns {string[]} */
const jsCommand = (handler) => [process.execPath, path.join(ADAPTER_DIR, 'yon-js-runner.js'), handler];

/**
 * Compiled languages share the yon-compiled-runner harness.
 * @param {string} language
 * @returns {(handler: string) => string[]}
 */
const compiledCommand = (language) => (handler) =>
    [process.execPath, path.join(ADAPTER_DIR, 'yon-compiled-runner.js'), language, handler];

/** @type {YonLanguageProvider[]} */
const BUILT_IN_PROVIDERS = [
    {
        language: 'javascript',
        extensions: ['.js', '.mjs', '.cjs'],
        shebangs: ['bun', 'node', 'deno'],
        command: jsCommand,
        hasHandlerClass: (source) => /\bexport\s+class\s+Handler\b/.test(source) || /\bclass\s+Handler\b/.test(source),
        hasMethod: (source, method) => new RegExp(`\\bstatic\\s+(?:async\\s+)?(?:\\*\\s*)?${method}\\s*\\(`).test(source),
    },
    {
        language: 'typescript',
        extensions: ['.ts', '.mts', '.cts'],
        command: jsCommand,
        hasHandlerClass: (source) => /\bexport\s+class\s+Handler\b/.test(source) || /\bclass\s+Handler\b/.test(source),
        hasMethod: (source, method) => new RegExp(`\\bstatic\\s+(?:async\\s+)?(?:\\*\\s*)?${method}\\s*\\(`).test(source),
    },
    {
        language: 'python',
        extensions: ['.py'],
        // Trailing '*' entries match as prefixes (python, python3, python3.12).
        shebangs: ['python*'],
        command: (handler) => ['python3', path.join(ADAPTER_DIR, 'yon-python-runner.py'), handler],
        hasHandlerClass: hasClassHandler,
        hasMethod: (source, method) => new RegExp(`\\bdef\\s+${method}\\s*\\(`).test(source),
    },
    {
        language: 'ruby',
        extensions: ['.rb'],
        shebangs: ['ruby'],
        command: (handler) => ['ruby', path.join(ADAPTER_DIR, 'yon-ruby-runner.rb'), handler],
        hasHandlerClass: hasClassHandler,
        hasMethod: (source, method) => new RegExp(`\\bdef\\s+self\\.${method}\\b`).test(source),
    },
    {
        language: 'php',
        extensions: ['.php'],
        shebangs: ['php'],
        command: (handler) => [phpExecutable(), path.join(ADAPTER_DIR, 'yon-php-runner.php'), handler],
        hasHandlerClass: hasClassHandler,
        hasMethod: (source, method) => new RegExp(`\\bstatic\\s+function\\s+${method}\\s*\\(`).test(source),
    },
    {
        language: 'dart',
        extensions: ['.dart'],
        shebangs: ['dart'],
        command: compiledCommand('dart'),
        hasHandlerClass: hasClassHandler,
        hasMethod: (source, method) => new RegExp(`\\bstatic\\s+[\\w<>,?\\s]+\\s+${method}\\s*\\(`).test(source),
    },
    {
        language: 'java',
        extensions: ['.java'],
        shebangs: ['java'],
        command: compiledCommand('java'),
        hasHandlerClass: hasClassHandler,
        hasMethod: (source, method) => new RegExp(`\\bstatic\\s+[\\w:<>,?\\[\\]&*\\s]+\\s+${method}\\s*\\(`).test(source),
    },
    {
        language: 'csharp',
        extensions: ['.cs'],
        shebangs: ['dotnet', 'csharp'],
        command: compiledCommand('csharp'),
        hasHandlerClass: hasClassHandler,
        hasMethod: (source, method) => new RegExp(`\\bstatic\\s+[\\w<>,?\\[\\]\\s]+\\s+${method}\\s*\\(`).test(source),
    },
    {
        language: 'cpp',
        extensions: ['.cpp', '.cc', '.cxx'],
        shebangs: ['clang++', 'g++', 'c++'],
        command: compiledCommand('cpp'),
        hasHandlerClass: hasClassHandler,
        hasMethod: (source, method) => new RegExp(`\\bstatic\\s+[\\w:<>,?\\[\\]&*\\s]+\\s+${method}\\s*\\(`).test(source),
    },
    {
        language: 'rust',
        extensions: ['.rs'],
        shebangs: ['rust', 'rustc'],
        command: compiledCommand('rust'),
        hasHandlerClass: (source) => /\b(?:struct|enum)\s+Handler\b/.test(source) && /\bimpl\s+Handler\b/.test(source),
        hasMethod: (source, method) => new RegExp(`\\b(?:pub\\s+)?fn\\s+${method}\\s*\\(`).test(source),
    },
];

/** Registration order matters for extension lookups: last registration wins. */
/** @type {Map<string, YonLanguageProvider>} */
const providersByLanguage = new Map(BUILT_IN_PROVIDERS.map((provider) => [provider.language, provider]));

/**
 * Register (or replace) a language provider.
 * @param {YonLanguageProvider} provider
 */
export function registerLanguageProvider(provider) {
    if (!provider || typeof provider.language !== 'string' || !provider.language)
        throw new Error('Language provider requires a non-empty "language" id');
    if (!Array.isArray(provider.extensions) || provider.extensions.length === 0)
        throw new Error(`Language provider '${provider.language}' requires at least one extension`);
    if (typeof provider.command !== 'function')
        throw new Error(`Language provider '${provider.language}' requires a command(handler) function`);
    if (typeof provider.hasHandlerClass !== 'function' || typeof provider.hasMethod !== 'function')
        throw new Error(`Language provider '${provider.language}' requires hasHandlerClass and hasMethod functions`);
    providersByLanguage.set(provider.language, provider);
}

/** @returns {string[]} All registered language ids, built-ins included. */
export function registeredLanguages() {
    return [...providersByLanguage.keys()];
}

/**
 * @param {string} language
 * @returns {YonLanguageProvider | null}
 */
export function providerForLanguage(language) {
    return providersByLanguage.get(language) ?? null;
}

/**
 * @param {string} extension Lowercase extension with dot.
 * @returns {YonLanguageProvider | null}
 */
export function providerForExtension(extension) {
    let match = null;
    for (const provider of providersByLanguage.values()) {
        if (provider.extensions.includes(extension))
            match = provider; // last registration wins
    }
    return match;
}

/**
 * @param {string} shebangCommand Basename of the shebang interpreter.
 * @returns {YonLanguageProvider | null}
 */
export function providerForShebang(shebangCommand) {
    if (!shebangCommand)
        return null;
    let match = null;
    for (const provider of providersByLanguage.values()) {
        for (const entry of provider.shebangs ?? []) {
            const matches = entry.endsWith('*')
                ? shebangCommand.startsWith(entry.slice(0, -1))
                : shebangCommand === entry;
            if (matches)
                match = provider; // last registration wins
        }
    }
    return match;
}
