// @ts-check
import { existsSync, readFileSync } from 'fs';
import path from 'path';

/**
 * @typedef {'javascript' | 'typescript' | 'python' | 'ruby' | 'php' | 'dart' | 'go' | 'java' | 'csharp' | 'rust'} YonHandlerLanguage
 *
 * @typedef {object} HandlerAdapterMatch
 * @property {YonHandlerLanguage} language
 * @property {string[]} command
 *
 */

const ADAPTER_DIR = path.join(import.meta.dir, 'adapters');

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
        'go',
        'java',
        'csharp',
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
        if (!language || !HandlerAdapter.hasHandlerConvention(source, language))
            return null;
        return {
            language,
            command: HandlerAdapter.commandFor(language, handler),
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
        return candidate && existsSync(candidate) ? candidate : 'php';
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
        if (extension === '.go')
            return 'go';
        if (extension === '.java')
            return 'java';
        if (extension === '.cs')
            return 'csharp';
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
        if (command === 'go')
            return 'go';
        if (command === 'java')
            return 'java';
        if (command === 'dotnet' || command === 'csharp')
            return 'csharp';
        if (command === 'rustc' || command === 'cargo')
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
     * @param {string} source
     * @param {YonHandlerLanguage} language
     * @returns {boolean}
     */
    static hasHandlerConvention(source, language) {
        if (language === 'javascript' || language === 'typescript') {
            return /\bexport\s+(?:async\s+)?function\s+handler\b/.test(source)
                || /\bexport\s+(?:const|let|var)\s+handler\b/.test(source)
                || /\bexport\s+default\s+(?:async\s+)?function\b/.test(source)
                || /\bexport\s+default\s+class\b/.test(source)
                || /\bhandler\s*\(/.test(source);
        }
        if (language === 'python') {
            return /^\s*(?:async\s+)?def\s+handler\s*\(/m.test(source)
                || /^\s*class\s+[A-Z][A-Z0-9_]*\b/m.test(source);
        }
        if (language === 'ruby') {
            return /^\s*def\s+handler\s*\(/m.test(source)
                || /^\s*class\s+[A-Z][A-Z0-9_]*\b/m.test(source);
        }
        if (language === 'php') {
            return /\bfunction\s+handler\s*\(/.test(source)
                || /\bclass\s+[A-Z][A-Z0-9_]*\b/.test(source);
        }
        if (language === 'dart') {
            return /\b(?:Future<[^>]+>\s+|Future\s+|dynamic\s+|Object\??\s+|Map<[^>]+>\s+)?handler\s*\(/.test(source)
                || /\bclass\s+[A-Z][A-Z0-9_]*\b/.test(source);
        }
        if (language === 'go') {
            return /\bfunc\s+Handler\s*\(/.test(source);
        }
        if (language === 'java' || language === 'csharp') {
            return /\bHandler\s*\(/.test(source) || /\bhandler\s*\(/.test(source);
        }
        if (language === 'rust') {
            return /\bfn\s+handler\s*\(/.test(source);
        }
        return false;
    }
}
