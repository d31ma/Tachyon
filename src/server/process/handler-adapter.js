// @ts-check
import { readFileSync } from 'fs';
import path from 'path';

/**
 * @typedef {'javascript' | 'typescript' | 'python' | 'ruby' | 'php' | 'dart' | 'java' | 'csharp' | 'cpp' | 'swift' | 'kotlin' | 'rust'} YonHandlerLanguage
 *
 * @typedef {object} HandlerAdapterMatch
 * @property {YonHandlerLanguage} language
 * @property {string[]} command
 * @property {Set<string>} methods
 *
 */

const ADAPTER_DIR = path.join(import.meta.dir, 'adapters');

/** HTTP methods that map to static method names on the Handler class. */
const HTTP_METHODS = new Set(['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS']);

/**
 * @param {string} commandPath
 * @returns {string}
 */
function basename(commandPath) {
    return commandPath.replaceAll('\\', '/').split('/').pop()?.toLowerCase() ?? commandPath.toLowerCase();
}

export default class HandlerAdapter {
    /** @type {ReadonlyArray<YonHandlerLanguage>} */
    static supportedLanguages = Object.freeze([
        'javascript',
        'typescript',
        'python',
        'ruby',
        'php',
        'dart',
        'java',
        'csharp',
        'cpp',
        'swift',
        'kotlin',
        'rust',
    ]);

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
        const language = HandlerAdapter.detectLanguage(handler, shebangTokens, source);
        if (!language || !HandlerAdapter.hasHandlerClass(source, language))
            return null;
        const methods = HandlerAdapter.detectMethods(source, language);
        if (methods.size === 0)
            return null;
        return {
            language,
            command: HandlerAdapter.commandFor(language, handler),
            methods,
        };
    }

    /**
     * @param {YonHandlerLanguage} language
     * @param {string} handler
     * @returns {string[]}
     */
    static commandFor(language, handler) {
        if (language === 'javascript' || language === 'typescript') {
            return [process.execPath, path.join(ADAPTER_DIR, 'yon-js-runner.js'), handler];
        }
        if (language === 'python') {
            return ['python3', path.join(ADAPTER_DIR, 'yon-python-runner.py'), handler];
        }
        if (language === 'ruby') {
            return ['ruby', path.join(ADAPTER_DIR, 'yon-ruby-runner.rb'), handler];
        }
        if (language === 'php') {
            return [HandlerAdapter.phpExecutable(), path.join(ADAPTER_DIR, 'yon-php-runner.php'), handler];
        }
        // Java, C#, Dart, C++, Swift, Kotlin, Rust — compiled languages
        return [process.execPath, path.join(ADAPTER_DIR, 'yon-compiled-runner.js'), language, handler];
    }

    /** @returns {string} */
    static phpExecutable() {
        if (process.platform !== 'win32')
            return 'php';
        const localAppData = process.env.LOCALAPPDATA;
        const candidate = localAppData
            ? path.join(localAppData, 'Microsoft', 'WinGet', 'Packages', 'PHP.PHP.8.4_Microsoft.Winget.Source_8wekyb3d8bbwe', 'php.exe')
            : '';
        return candidate && readFileSync.length && candidate ? candidate : 'php';
    }

    /**
     * @param {string} handler
     * @param {string[]} shebangTokens
     * @param {string} source
     * @returns {YonHandlerLanguage | null}
     */
    static detectLanguage(handler, shebangTokens, source) {
        const extension = path.extname(handler).toLowerCase();
        if (extension === '.js' || extension === '.mjs' || extension === '.cjs')
            return 'javascript';
        if (extension === '.ts' || extension === '.mts' || extension === '.cts')
            return 'typescript';
        if (extension === '.py')
            return 'python';
        if (extension === '.rb')
            return 'ruby';
        if (extension === '.php')
            return 'php';
        if (extension === '.dart')
            return 'dart';
        if (extension === '.java')
            return 'java';
        if (extension === '.cs')
            return 'csharp';
        if (extension === '.cpp' || extension === '.cc' || extension === '.cxx')
            return 'cpp';
        if (extension === '.swift')
            return 'swift';
        if (extension === '.kt' || extension === '.kts')
            return 'kotlin';
        if (extension === '.rs')
            return 'rust';

        const command = basename(shebangTokens[0] ?? '');
        if (['bun', 'node', 'deno'].includes(command))
            return source.includes(':') && /\bexport\s+(?:async\s+)?function\s+handler\b/.test(source)
                ? 'typescript'
                : 'javascript';
        if (command.startsWith('python'))
            return 'python';
        if (command === 'ruby')
            return 'ruby';
        if (command === 'php')
            return 'php';
        if (command === 'dart')
            return 'dart';
        if (command === 'java')
            return 'java';
        if (command === 'dotnet' || command === 'csharp')
            return 'csharp';
        if (command === 'clang++' || command === 'g++' || command === 'c++')
            return 'cpp';
        if (command === 'swift' || command === 'swiftc')
            return 'swift';
        if (command === 'kotlin' || command === 'kotlinc')
            return 'kotlin';
        if (command === 'rust' || command === 'rustc')
            return 'rust';
        return null;
    }

    /**
     * @param {unknown} value
     * @returns {value is YonHandlerLanguage}
     */
    static isSupportedLanguage(value) {
        return typeof value === 'string'
            && HandlerAdapter.supportedLanguages.includes(/** @type {YonHandlerLanguage} */ (value));
    }

    /**
     * Checks whether the source follows the `class Handler` convention.
     * Each supported language must define a class named `Handler` with at
     * least one static method whose name matches an HTTP verb.
     * @param {string} source
     * @param {YonHandlerLanguage} language
     * @returns {boolean}
     */
    static hasHandlerConvention(source, language) {
        if (!HandlerAdapter.hasHandlerClass(source, language))
            return false;
        return HandlerAdapter.detectMethods(source, language).size > 0;
    }

    /**
     * Checks whether the source declares a `Handler` class.
     * @param {string} source
     * @param {YonHandlerLanguage} language
     * @returns {boolean}
     */
    static hasHandlerClass(source, language) {
        if (language === 'javascript' || language === 'typescript') {
            return /\bexport\s+class\s+Handler\b/.test(source)
                || /\bclass\s+Handler\b/.test(source);
        }
        // Swift handlers may namespace static methods under a class, struct, or enum.
        if (language === 'swift') {
            return /\b(?:class|struct|enum)\s+Handler\b/.test(source);
        }
        // Kotlin handlers expose HTTP verbs through a `class Handler { companion object }`
        // or a top-level `object Handler`.
        if (language === 'kotlin') {
            return /\b(?:class|object)\s+Handler\b/.test(source);
        }
        if (language === 'rust') {
            return /\b(?:struct|enum)\s+Handler\b/.test(source) && /\bimpl\s+Handler\b/.test(source);
        }
        // Python, Ruby, PHP, Dart, Java, C#, C++ all use `class Handler`
        return /\bclass\s+Handler\b/.test(source);
    }

    /**
     * Scans source for HTTP method names implemented as static methods on
     * the `Handler` class. Returns the set of methods found.
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
        if (language === 'javascript' || language === 'typescript') {
            // static GET(request), static async GET(request), or static async *GET() (generator)
            return new RegExp(`\\bstatic\\s+(?:async\\s+)?(?:\\*\\s*)?${method}\\s*\\(`).test(source);
        }
        if (language === 'python') {
            // @staticmethod\n    def GET(request) or def GET(request) inside class
            return new RegExp(`\\bdef\\s+${method}\\s*\\(`).test(source);
        }
        if (language === 'ruby') {
            // def self.GET(request)
            return new RegExp(`\\bdef\\s+self\\.${method}\\b`).test(source);
        }
        if (language === 'php') {
            // public static function GET($request)
            return new RegExp(`\\bstatic\\s+function\\s+${method}\\s*\\(`).test(source);
        }
        if (language === 'dart') {
            // static dynamic GET(Map<String, dynamic> request) or static Future<...> GET(...)
            return new RegExp(`\\bstatic\\s+[\\w<>,?\\s]+\\s+${method}\\s*\\(`).test(source);
        }
        if (language === 'java' || language === 'cpp') {
            // Java: public static Object GET(Map<String, Object> request)
            // C++: static YonJson GET(const YonJson& request)
            return new RegExp(`\\bstatic\\s+[\\w:<>,?\\[\\]&*\\s]+\\s+${method}\\s*\\(`).test(source);
        }
        if (language === 'csharp') {
            // public static Dictionary<string, object?> GET(JsonElement request)
            return new RegExp(`\\bstatic\\s+[\\w<>,?\\[\\]\\s]+\\s+${method}\\s*\\(`).test(source);
        }
        if (language === 'swift') {
            // static func GET(_ request: [String: Any]) -> Any?
            return new RegExp(`\\bstatic\\s+func\\s+${method}\\s*\\(`).test(source);
        }
        if (language === 'kotlin') {
            // fun GET(request: Map<String, Any?>): Any? inside `companion object` / `object Handler`
            return new RegExp(`\\bfun\\s+${method}\\s*\\(`).test(source);
        }
        if (language === 'rust') {
            // impl Handler { pub fn GET(request: &YonJson) -> YonJson { ... } }
            return new RegExp(`\\b(?:pub\\s+)?fn\\s+${method}\\s*\\(`).test(source);
        }
        return false;
    }
}
