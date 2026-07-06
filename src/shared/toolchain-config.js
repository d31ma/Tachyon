// @ts-check
//
// Declarative toolchain configuration loader.
//
// Reads `.tachyonrc` from the project root and merges it with sensible
// defaults. Consumed by `yon-compiled-runner.js`.

import { existsSync, readFileSync } from 'fs';
import path from 'path';

/**
 * @typedef {object} ToolchainEntry
 * @property {string} [compiler]
 * @property {string} [edition]
 * @property {string} [standard]
 * @property {string} [runtime]
 * @property {string} [sdk]
 * @property {string} [framework]
 * @property {string[]} [flags]
 */

/**
 * @typedef {object} ToolchainConfig
 * @property {Partial<Record<string, ToolchainEntry>>} toolchains
 * @property {Record<string, string[]>} interpreters File-extension → argv
 *   prefix for running a `yon.<ext>` handler directly (`.go` → `go run`).
 *   Not a closed list: seeded with common languages, and any extension can
 *   be added or overridden in `.tachyonrc`.
 */

/** @type {ToolchainConfig | null} */
let cachedConfig = null;

/**
 * Default extension → run-command map for the universal handler path.
 * These let `server/routes/foo/yon.<ext>` run by extension, no shebang — the
 * only requirement is that the tool is on the machine. Extend or override
 * per project via `.tachyonrc` "interpreters".
 * @returns {Record<string, string[]>}
 */
export function defaultInterpreters() {
    return {
        '.js': ['bun'], '.mjs': ['bun'], '.cjs': ['bun'], '.ts': ['bun'],
        '.py': ['python3'], '.rb': ['ruby'], '.php': ['php'],
        '.pl': ['perl'], '.lua': ['lua'], '.r': ['Rscript'],
        '.go': ['go', 'run'], '.zig': ['zig', 'run'],
        '.exs': ['elixir'], '.jl': ['julia'], '.swift': ['swift'],
        '.sh': ['sh'], '.bash': ['bash'], '.ps1': ['pwsh', '-File'],
        '.groovy': ['groovy'], '.raku': ['raku'],
    };
}

/** @returns {ToolchainConfig} */
export function defaultToolchainConfig() {
    return {
        toolchains: {
            rust: {
                compiler: 'rustc',
                edition: '2021',
                // Statically link libstd: a compiled handler runs as a
                // standalone subprocess, so it must not depend on the Rust
                // toolchain's dynamic libstd being on the runtime library
                // path (it is not on Linux — `prefer-dynamic` produced a
                // binary that failed with "libstd-*.so: cannot open shared
                // object file"). Override via TACHYON_RUSTFLAGS if needed.
                flags: [],
            },
            cpp: {
                compiler: '',
                standard: 'c++17',
                flags: [],
            },
            java: {
                compiler: 'javac',
                runtime: 'java',
                flags: [],
            },
            csharp: {
                sdk: 'dotnet',
                framework: 'net8.0',
                flags: [],
            },
            dart: {
                compiler: 'dart',
                flags: [],
            },
        },
        interpreters: defaultInterpreters(),
    };
}

/**
 * Load and cache `.tachyonrc` from the project root.
 * @param {string} [projectRoot]
 * @returns {ToolchainConfig}
 */
export function loadToolchainConfig(projectRoot = process.cwd()) {
    if (cachedConfig) return cachedConfig;

    const defaults = defaultToolchainConfig();
    const rcPath = path.join(projectRoot, '.tachyonrc');

    if (!existsSync(rcPath)) {
        cachedConfig = defaults;
        return cachedConfig;
    }

    let userConfig;
    try {
        userConfig = JSON.parse(readFileSync(rcPath, 'utf8'));
    } catch (error) {
        throw new Error(
            `Failed to parse .tachyonrc: ${error instanceof Error ? error.message : String(error)}`
        );
    }

    if (userConfig.toolchains && typeof userConfig.toolchains === 'object') {
        for (const [lang, entry] of Object.entries(userConfig.toolchains)) {
            if (entry && typeof entry === 'object') {
                const existing = defaults.toolchains[lang] ?? {};
                defaults.toolchains[lang] = { ...existing, ...entry };
            }
        }
    }

    if (userConfig.interpreters && typeof userConfig.interpreters === 'object') {
        for (const [ext, command] of Object.entries(userConfig.interpreters)) {
            // Accept "go run" or ["go", "run"]; normalise the extension key.
            const argv = typeof command === 'string'
                ? command.trim().split(/\s+/).filter(Boolean)
                : Array.isArray(command) ? command.map(String) : [];
            const key = ext.startsWith('.') ? ext.toLowerCase() : `.${ext.toLowerCase()}`;
            if (argv.length > 0)
                defaults.interpreters[key] = argv;
            else
                delete defaults.interpreters[key];
        }
    }

    cachedConfig = defaults;
    return cachedConfig;
}

/**
 * Resolve the run command for a handler extension (`.go` → `['go','run']`),
 * or null when no interpreter is configured for it.
 * @param {string} extension Handler file extension, with or without a dot.
 * @param {ToolchainConfig} [config]
 * @returns {string[] | null}
 */
export function resolveInterpreter(extension, config = loadToolchainConfig()) {
    if (!extension) return null;
    const key = extension.startsWith('.') ? extension.toLowerCase() : `.${extension.toLowerCase()}`;
    const argv = config.interpreters?.[key];
    return argv && argv.length > 0 ? [...argv] : null;
}

/** Clear the cached config (useful in tests / HMR). */
export function clearToolchainConfig() {
    cachedConfig = null;
}

/**
 * Resolve the effective compiler command and flags for a language.
 * Falls back to the hard-coded executable name when `.tachyonrc` does not
 * specify one.
 * @param {string} language
 * @param {ToolchainConfig} config
 * @returns {{ command: string, flags: string[] }}
 */
export function resolveCompiler(language, config) {
    const entry = config.toolchains[language];
    if (!entry) {
        return { command: '', flags: [] };
    }

    // Flags from env override flags from config (Stage 2 contract)
    const envKey = language === 'cpp' ? 'TACHYON_CXXFLAGS' : language === 'rust' ? 'TACHYON_RUSTFLAGS' : null;
    if (envKey && process.env[envKey] !== undefined) {
        const envFlags = process.env[envKey].trim();
        entry.flags = envFlags ? envFlags.split(/\s+/) : [];
    }

    return {
        command: entry.compiler ?? '',
        flags: entry.flags ?? [],
    };
}

/**
 * Return a human-readable description of the resolved toolchain.
 * @param {string} language
 * @param {ToolchainConfig} config
 * @returns {string}
 */
export function describeToolchain(language, config) {
    const { command, flags } = resolveCompiler(language, config);
    if (!command) return `${language}: (not configured)`;
    const flagStr = flags.length > 0 ? ` (flags: ${flags.join(' ')})` : '';
    return `${language}: ${command}${flagStr}`;
}
