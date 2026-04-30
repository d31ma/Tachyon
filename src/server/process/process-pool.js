// @ts-check
import { existsSync, readFileSync } from 'fs';
import Router from "../http/route-handler.js";
import HandlerAdapter from './handler-adapter.js';
import logger from '../observability/logger.js';
/** Maximum time (ms) a handler process may run before it is killed. Default: 30 s. */
const HANDLER_TIMEOUT_MS = process.env.YON_HANDLER_TIMEOUT_MS
    ? Number(process.env.YON_HANDLER_TIMEOUT_MS)
    : 30_000;
const poolLogger = logger.child({ scope: 'process-pool' });

/**
 * @param {string} value
 * @returns {string[]}
 */
function tokenizeCommand(value) {
    const tokens = value.match(/"[^"]*"|'[^']*'|\S+/g) ?? [];
    return tokens.map((token) => token.replace(/^['"]|['"]$/g, ''));
}

/**
 * @param {string} commandPath
 * @returns {string}
 */
function basename(commandPath) {
    return commandPath.replaceAll('\\', '/').split('/').pop() ?? commandPath;
}

export default class Pool {
    /**
     * Pre-warmed handler processes keyed by absolute handler path.
     * A process is spawned ahead of time so it has already loaded its interpreter
     * and is blocking on stdin by the time a request arrives, eliminating
     * cold-start latency from the request path.
     */
    static warmedProcesses = new Map();

    /**
     * Resolves route modules through Yon adapters first. Shebangs remain a
     * legacy fallback for directly executable handler files.
     * @param {string} handler
     * @returns {string[]}
     */
    static resolveHandlerCommand(handler) {
        let firstLine = '';
        try {
            firstLine = readFileSync(handler, 'utf8').split(/\r?\n/, 1)[0] ?? '';
        }
        catch {
            return [handler];
        }
        let tokens = firstLine.startsWith('#!')
            ? tokenizeCommand(firstLine.slice(2).trim())
            : [];
        if (tokens.length === 0) {
            const adapter = HandlerAdapter.resolve(handler, tokens);
            return adapter ? adapter.command : [handler];
        }
        const launcher = basename(tokens[0]).toLowerCase();
        if (launcher === 'env') {
            tokens = tokens.slice(1);
            if (tokens[0] === '-S')
                tokens = tokens.slice(1);
        }
        const adapter = HandlerAdapter.resolve(handler, tokens);
        if (adapter)
            return adapter.command;
        const [command, ...args] = tokens;
        if (!command)
            return [handler];
        if (basename(command).toLowerCase() === 'bun')
            return [process.execPath, ...args, handler];
        return [basename(command), ...args, handler];
    }
    /**
     * Pre-spawns a handler process so it is warmed and blocking on stdin
     * before the next request arrives. Called at startup and after each HMR reload.
     * @param {string} handler - Absolute path to the handler executable
     */
    static prewarmHandler(handler) {
        if (Pool.warmedProcesses.has(handler))
            return;
        const cmd = Pool.resolveHandlerCommand(handler);
        poolLogger.debug('Prewarming handler process', { handler });
        try {
            Pool.warmedProcesses.set(handler, Bun.spawn({
                cmd,
                stdin: "pipe",
                stdout: "pipe",
                stderr: "pipe",
                env: process.env,
            }));
        }
        catch (error) {
            poolLogger.warn('Skipping handler prewarm because its runtime is unavailable', {
                handler,
                cmd,
                err: error,
            });
        }
    }
    /**
     * Pre-warms one process for every non-OPTIONS handler discovered
     * by {@link Router.validateRoutes}. Call this after `createServerRoutes`.
     */
    static prewarmAllHandlers() {
        for (const [route, methods] of Router.allRoutes) {
            for (const method of methods) {
                if (method === 'OPTIONS')
                    continue;
                const handler = Router.routeHandlers[route]?.[method]
                    ?? `${Router.routesPath}${route === '/' ? '' : route}/${method}`;
                if (existsSync(handler))
                    Pool.prewarmHandler(handler);
            }
        }
    }
    /**
     * Clears all warmed processes. Must be called before an HMR reload so
     * stale processes (pointing at old handler files) are discarded.
     */
    static clearWarmedProcesses() {
        for (const proc of Pool.warmedProcesses.values()) {
            try {
                proc.kill();
            }
            catch { /* already exited */ }
        }
        Pool.warmedProcesses.clear();
    }
    /**
     * Returns the pre-warmed process for `handler` if one exists and is still
     * running, otherwise spawns a fresh process. Immediately schedules a
     * replacement warm process for the next request.
     *
     * A kill-on-timeout timer is armed: if the process has not exited within
     * HANDLER_TIMEOUT_MS it is killed and the event is logged.
     */
    /**
     * @param {string} handler
     */
    static acquireHandler(handler) {
        const warmed = Pool.warmedProcesses.get(handler);
        Pool.warmedProcesses.delete(handler);
        // Schedule a replacement so the next request finds a warm process
        setImmediate(() => Pool.prewarmHandler(handler));
        const cmd = Pool.resolveHandlerCommand(handler);
        // If the warmed process exited early (e.g. handler syntax error), spawn fresh
        const proc = (warmed && warmed.exitCode === null)
            ? warmed
            : Bun.spawn({
                cmd,
                stdin: "pipe",
                stdout: "pipe",
                stderr: "pipe",
                env: process.env,
            });
        if (warmed && warmed.exitCode === null) {
            poolLogger.debug('Using prewarmed handler process', { handler, pid: proc.pid });
        }
        else {
            poolLogger.debug('Spawned fresh handler process', { handler, pid: proc.pid });
        }
        // Kill hung processes to prevent resource exhaustion
        const timeout = setTimeout(() => {
            if (proc.exitCode === null) {
                poolLogger.error('Handler timed out and will be killed', {
                    handler,
                    pid: proc.pid,
                    timeoutMs: HANDLER_TIMEOUT_MS,
                });
                try {
                    proc.kill();
                }
                catch { /* already exited */ }
            }
        }, HANDLER_TIMEOUT_MS);
        // Clear the timer once the process exits naturally
        proc.exited.then(() => clearTimeout(timeout)).catch(() => clearTimeout(timeout));
        return proc;
    }
}
