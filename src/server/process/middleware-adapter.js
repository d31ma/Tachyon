// @ts-check
import { access, readdir, stat } from 'fs/promises';
import { accessSync, constants, readFileSync } from 'fs';
import path from 'path';
import { providerForExtension, providerForShebang } from './language-providers.js';
import { resolveInterpreter } from '../../shared/toolchain-config.js';
import logger from '../observability/logger.js';

/**
 * @typedef {import('../http/route-handler.js').Middleware} Middleware
 * @typedef {import('../http/route-handler.js').RateLimiter} RateLimiter
 * @typedef {import('../http/route-handler.js').RateLimitDecision} RateLimitDecision
 * @typedef {import('../http/route-handler.js').RequestContext} RequestContext
 * @typedef {{ status?: number, statusText?: string, headers?: Record<string, string>, body?: unknown }} MiddlewareResponseEnvelope
 * @typedef {'before' | 'after' | 'rateLimit'} MiddlewarePhase
 * @typedef {{
 *   action?: 'continue' | 'respond' | 'replace',
 *   status?: number,
 *   statusText?: string,
 *   headers?: Record<string, string>,
 *   body?: unknown,
 *   response?: MiddlewareResponseEnvelope
 * }} MiddlewarePhaseResult
 */

const middlewareLogger = logger.child({ scope: 'middleware' });
const RESPONSE_START = '\x1fTACHYON_RESPONSE\x1e';
const RESPONSE_END = '\x1eTACHYON_RESPONSE\x1f';
const CLASS_STYLE_LANGUAGES = new Set(['javascript', 'typescript', 'python', 'ruby', 'php']);
const PHASES = new Set(['before', 'after', 'rateLimit']);

/** @param {string} commandPath */
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
 * @param {string} value
 * @returns {string[]}
 */
function tokenizeCommand(value) {
    const tokens = value.match(/"[^"]*"|'[^']*'|\S+/g) ?? [];
    return tokens.map((token) => token.replace(/^['"]|['"]$/g, ''));
}

/**
 * @param {string} filePath
 * @returns {Promise<boolean>}
 */
async function pathExists(filePath) {
    try {
        await access(filePath);
        return true;
    }
    catch {
        return false;
    }
}

/**
 * @param {string} filePath
 * @returns {Promise<boolean>}
 */
async function isDirectory(filePath) {
    try {
        return (await stat(filePath)).isDirectory();
    }
    catch {
        return false;
    }
}

/**
 * @param {unknown} value
 * @returns {Record<string, string>}
 */
function headersObject(value) {
    if (!value || typeof value !== 'object' || Array.isArray(value))
        return {};
    /** @type {Record<string, string>} */
    const headers = {};
    for (const [name, entry] of Object.entries(value)) {
        if (entry !== undefined && entry !== null)
            headers[name] = String(entry);
    }
    return headers;
}

/** @param {unknown} body */
function serializeBody(body) {
    if (body === undefined || body === null)
        return null;
    if (typeof body === 'string' || body instanceof Uint8Array || body instanceof ReadableStream)
        return body;
    return JSON.stringify(body);
}

/**
 * @param {unknown} value
 * @returns {Record<string, unknown> | null}
 */
function objectOrNull(value) {
    return value && typeof value === 'object' && !Array.isArray(value)
        ? /** @type {Record<string, unknown>} */ (value)
        : null;
}

/**
 * @param {Response} response
 * @returns {{ status: number, statusText: string, headers: Record<string, string> }}
 */
function responseSnapshot(response) {
    return {
        status: response.status,
        statusText: response.statusText,
        headers: Object.fromEntries(response.headers.entries()),
    };
}

/**
 * @param {Request} request
 * @returns {{ method: string, url: string, headers: Record<string, string> }}
 */
function requestSnapshot(request) {
    return {
        method: request.method,
        url: request.url,
        headers: Object.fromEntries(request.headers.entries()),
    };
}

export default class MiddlewareAdapter {
    /** @type {string} */
    handler;
    /** @type {string[]} */
    command;
    /** @type {'class' | 'raw'} */
    mode;
    /** @type {Set<MiddlewarePhase>} */
    phases;

    /**
     * @param {{ handler: string, command: string[], mode: 'class' | 'raw', phases?: Set<MiddlewarePhase> }} options
     */
    constructor(options) {
        this.handler = options.handler;
        this.command = options.command;
        this.mode = options.mode;
        this.phases = options.phases ?? new Set(/** @type {MiddlewarePhase[]} */ ([...PHASES]));
    }

    /**
     * @param {string} configuredPath
     * @param {string} [root]
     * @returns {Promise<MiddlewareAdapter | null>}
     */
    static async discover(configuredPath, root = process.cwd()) {
        const configured = path.isAbsolute(configuredPath)
            ? configuredPath
            : path.join(root, configuredPath);
        const candidates = [
            configured,
            path.join(root, 'server', 'middleware'),
        ];
        for (const candidate of candidates) {
            const handler = await MiddlewareAdapter.findHandler(candidate);
            if (!handler)
                continue;
            const resolved = MiddlewareAdapter.resolve(handler);
            if (resolved)
                return new MiddlewareAdapter({ handler, ...resolved });
            throw new Error(
                `Invalid polyglot middleware '${handler}' — define a class named Middleware with static ` +
                `before/after/rateLimit methods for JavaScript, TypeScript, Python, Ruby or PHP, or make it ` +
                `a raw protocol executable/interpreted file that reads one JSON envelope from stdin and writes JSON to stdout.`
            );
        }
        return null;
    }

    /**
     * @param {string} candidate
     * @returns {Promise<string | null>}
     */
    static async findHandler(candidate) {
        if (await pathExists(candidate) && !(await isDirectory(candidate)))
            return candidate;
        if (!(await isDirectory(candidate)))
            return null;
        const entries = await readdir(candidate, { withFileTypes: true });
        const match = entries
            .filter((entry) => entry.isFile() && /^yon\.[^.]+$/.test(entry.name))
            .map((entry) => entry.name)
            .sort()[0];
        return match ? path.join(candidate, match) : null;
    }

    /**
     * @param {string} handler
     * @returns {{ command: string[], mode: 'class' | 'raw', phases?: Set<MiddlewarePhase> } | null}
     */
    static resolve(handler) {
        let source = '';
        try {
            source = readFileSync(handler, 'utf8');
        }
        catch {
            return null;
        }
        const shebangTokens = source.startsWith('#!')
            ? tokenizeCommand(source.split(/\r?\n/, 1)[0].slice(2).trim())
            : [];
        const extension = path.extname(handler).toLowerCase();
        const provider = (extension ? providerForExtension(extension) : null)
            ?? providerForShebang(basename(shebangTokens[0] ?? ''));
        if (provider && CLASS_STYLE_LANGUAGES.has(provider.language) && /\bclass\s+Middleware\b/.test(source)) {
            const phases = new Set(
                /** @type {MiddlewarePhase[]} */ ([...PHASES]).filter((phase) => provider.hasMethod(source, phase)),
            );
            return phases.size > 0 ? { command: provider.command(handler), mode: 'class', phases } : null;
        }
        const interpreter = resolveInterpreter(extension);
        if (interpreter || isExecutableFile(handler)) {
            return { command: interpreter ? [...interpreter, handler] : [handler], mode: 'raw' };
        }
        return null;
    }

    /**
     * @param {string} output
     * @returns {string}
     */
    static responseBody(output) {
        const start = output.indexOf(RESPONSE_START);
        if (start === -1)
            return output;
        const bodyStart = start + RESPONSE_START.length;
        const end = output.indexOf(RESPONSE_END, bodyStart);
        return end === -1 ? output : output.slice(bodyStart, end);
    }

    /**
     * @param {string} text
     * @returns {unknown}
     */
    static parseOutput(text) {
        const body = MiddlewareAdapter.responseBody(text).trim();
        if (!body)
            return null;
        return JSON.parse(body);
    }

    /**
     * @param {MiddlewarePhaseResult | null} result
     * @returns {Response | null}
     */
    static responseFromBeforeResult(result) {
        if (!result || result.action === 'continue')
            return null;
        const envelope = result.action === 'respond'
            ? (objectOrNull(result.response) ?? result)
            : result;
        return MiddlewareAdapter.responseFromEnvelope(envelope);
    }

    /**
     * @param {Response} original
     * @param {MiddlewarePhaseResult | null} result
     * @returns {Response}
     */
    static responseFromAfterResult(original, result) {
        if (!result || result.action === 'continue')
            return original;
        if (result.action === 'replace' || result.status !== undefined || result.body !== undefined) {
            const envelope = objectOrNull(result.response) ?? result;
            return MiddlewareAdapter.responseFromEnvelope(envelope, original);
        }
        const headers = new Headers(original.headers);
        for (const [name, value] of Object.entries(headersObject(result.headers))) {
            headers.set(name, value);
        }
        return new Response(original.body, {
            status: original.status,
            statusText: original.statusText,
            headers,
        });
    }

    /**
     * @param {Record<string, unknown>} envelope
     * @param {Response} [fallback]
     * @returns {Response}
     */
    static responseFromEnvelope(envelope, fallback) {
        const headers = new Headers(fallback?.headers);
        for (const [name, value] of Object.entries(headersObject(envelope.headers))) {
            headers.set(name, value);
        }
        return new Response(serializeBody(envelope.body), {
            status: Number(envelope.status ?? fallback?.status ?? 200),
            statusText: String(envelope.statusText ?? fallback?.statusText ?? ''),
            headers,
        });
    }

    /**
     * @param {unknown} value
     * @returns {RateLimitDecision | null}
     */
    static rateLimitDecision(value) {
        const candidate = objectOrNull(value);
        if (!candidate)
            return null;
        if (typeof candidate.allowed !== 'boolean')
            return null;
        const now = Date.now();
        return {
            allowed: candidate.allowed,
            limit: Number(candidate.limit ?? 0),
            remaining: Number(candidate.remaining ?? 0),
            resetAt: Number(candidate.resetAt ?? now),
            headers: headersObject(candidate.headers),
        };
    }

    /**
     * @param {MiddlewarePhase} phase
     * @param {Record<string, unknown>} payload
     * @returns {Promise<unknown>}
     */
    async execute(phase, payload) {
        if (!PHASES.has(phase))
            throw new Error(`Unsupported middleware phase '${phase}'`);
        const stdin = {
            ...(this.mode === 'class' ? { className: 'Middleware', method: phase } : { phase }),
            ...payload,
        };
        const proc = Bun.spawn({
            cmd: this.command,
            stdin: 'pipe',
            stdout: 'pipe',
            stderr: 'pipe',
            env: process.env,
        });
        const timeoutMs = Number(process.env.YON_MIDDLEWARE_TIMEOUT_MS || process.env.YON_HANDLER_TIMEOUT_MS || 30_000);
        const timeout = setTimeout(() => {
            try {
                proc.kill();
            }
            catch { /* already exited */ }
        }, timeoutMs);
        proc.stdin.write(JSON.stringify(stdin));
        proc.stdin.end();
        const [stdout, stderr, exitCode] = await Promise.all([
            new Response(proc.stdout).text(),
            new Response(proc.stderr).text(),
            proc.exited,
        ]);
        clearTimeout(timeout);
        if (stderr.trim()) {
            middlewareLogger.warn('Middleware stderr output', {
                handler: this.handler,
                phase,
                output: stderr.trim(),
            });
        }
        if (exitCode !== 0) {
            throw new Error(`Middleware '${this.handler}' failed during ${phase}: ${stderr.trim() || `exit ${exitCode}`}`);
        }
        return MiddlewareAdapter.parseOutput(stdout);
    }

    /**
     * @returns {{ middleware: Middleware | null, rateLimiter: RateLimiter | null }}
     */
    toRuntimeHooks() {
        /** @type {Middleware} */
        const middleware = {};
        if (this.phases.has('before')) {
            middleware.before = async (request, context) => {
                const result = /** @type {MiddlewarePhaseResult | null} */ (await this.execute('before', {
                    request: requestSnapshot(request),
                    context,
                }));
                return MiddlewareAdapter.responseFromBeforeResult(result) ?? undefined;
            };
        }
        if (this.phases.has('after')) {
            middleware.after = async (request, response, context) => {
                const result = /** @type {MiddlewarePhaseResult | null} */ (await this.execute('after', {
                    request: requestSnapshot(request),
                    response: responseSnapshot(response),
                    context,
                }));
                return MiddlewareAdapter.responseFromAfterResult(response, result);
            };
        }
        /** @type {RateLimiter | null} */
        const rateLimiter = this.phases.has('rateLimit')
            ? {
                take: async (request, context) => MiddlewareAdapter.rateLimitDecision(await this.execute('rateLimit', {
                    request: requestSnapshot(request),
                    context,
                })),
            }
            : null;
        return { middleware, rateLimiter };
    }
}
