#!/usr/bin/env bun
// @ts-check
import Yon from "../server/yon.js";
import Pool from "../server/process-pool.js";
import Router from "../server/route-handler.js";
import Tac from "../compiler/template-compiler.js";
import logger from "../server/logger.js";
import { watch } from "fs";
import { access, readdir, stat } from "fs/promises";
import path from "path";
import { serveStaticPreviewRequest } from "../runtime/static-preview.js";

/**
 * @typedef {import("bun").Server<any>} BunServer
 * @typedef {import("fs").FSWatcher} FSWatcher
 * @typedef {ReadableStreamDefaultController<string>} HmrClient
 */

/** Debounce delay (ms) applied to file-watcher events before triggering an HMR reload */
const HMR_DEBOUNCE_MS = 1000;
const bundleWatchEnabled = process.argv.includes('--bundle-watch');
const start = Date.now();
let bundleWatcher = null;
const distPath = path.join(process.cwd(), 'dist');
const bundleCliPath = `${import.meta.dir}/bundle.js`;
const hotReloadClientPath = path.join(import.meta.dir, '../runtime/hot-reload-client.js');
const serveLogger = logger.child({ scope: 'cli:serve' });
/** @type {Set<HmrClient>} */
const hmrClients = new Set();
const hmrMaxClients = Number(process.env.HMR_MAX_CLIENTS) || 20;
/** @type {FSWatcher[]} */
const hmrWatchers = [];
let hmrWatchersStarted = false;
/** @param {string} pathname */
function isAssetRequest(pathname) {
    return path.posix.basename(pathname).includes('.');
}
/** @param {Request} request */
function wantsHtmlDocument(request) {
    return Yon.isDocumentRequest(request);
}
/** @param {string} path */
async function pathExists(path) {
    try {
        await access(path);
        return true;
    }
    catch {
        return false;
    }
}
/**
 * @param {string} root
 * @returns {Promise<boolean>}
 */
async function directoryHasFiles(root) {
    try {
        const info = await stat(root);
        if (!info.isDirectory())
            return false;
    }
    catch {
        return false;
    }
    for (const entry of await readdir(root, { withFileTypes: true })) {
        if (entry.name === '.gitkeep')
            continue;
        const entryPath = path.join(root, entry.name);
        if (entry.isFile())
            return true;
        if (entry.isDirectory() && await directoryHasFiles(entryPath))
            return true;
    }
    return false;
}
/**
 * @returns {Promise<{ frontend: boolean, backend: boolean, mode: 'frontend' | 'backend' | 'full' | 'empty' }>}
 */
async function detectAppShape() {
    const frontend = await directoryHasFiles(path.join(process.cwd(), 'browser'));
    const backend = await directoryHasFiles(path.join(process.cwd(), 'server'));
    return {
        frontend,
        backend,
        mode: frontend && backend
            ? 'full'
            : frontend
                ? 'frontend'
                : backend
                    ? 'backend'
                    : 'empty',
    };
}
async function loadMiddleware() {
    Router.middleware = null;
    Router.rateLimiter = null;
    const filePath = `${Router.middlewarePath}.js`;
    if (await pathExists(filePath)) {
        const mod = await import(filePath);
        const loaded = mod.default ?? mod;
        if (typeof loaded !== 'object'
            || loaded === null
            || (loaded.before !== undefined && typeof loaded.before !== 'function')
            || (loaded.after !== undefined && typeof loaded.after !== 'function')
            || (loaded.rateLimiter !== undefined
                && (typeof loaded.rateLimiter !== 'object'
                    || loaded.rateLimiter === null
                    || typeof loaded.rateLimiter.take !== 'function'))) {
            throw new Error(`Middleware at '${filePath}' must export an object with optional before/after functions and optional rateLimiter.take(request, context)`);
        }
        const middlewareModule = loaded;
        Router.middleware = middlewareModule.before || middlewareModule.after
            ? { before: middlewareModule.before, after: middlewareModule.after }
            : null;
        Router.rateLimiter = middlewareModule.rateLimiter ?? null;
    }
}
const appShape = await detectAppShape();
const frontendEnabled = appShape.frontend;
const backendEnabled = appShape.backend;
const hmrEnabled = frontendEnabled || bundleWatchEnabled;
async function configureRoutes(isReload = false) {
    Router.resetStaticState();
    if (isReload)
        Pool.clearWarmedProcesses();
    if (backendEnabled) {
        await loadMiddleware();
        await Router.validateRoutes();
        Yon.createServerRoutes();
        Pool.prewarmAllHandlers();
    }
    if (frontendEnabled) {
        await Router.validatePageRoutes();
    }
}
if (frontendEnabled) {
    const { runBuild } = await import('./bundle.js');
    await runBuild();
    Router.resetStaticState();
}
await configureRoutes();
if (hmrEnabled) {
    const bundleArgs = frontendEnabled
        ? ['bun', bundleCliPath, '--watch', '--skip-initial-build']
        : ['bun', bundleCliPath, '--watch'];
    bundleWatcher = Bun.spawn(bundleArgs, {
        cwd: process.cwd(),
        stdout: 'inherit',
        stderr: 'inherit'
    });
}
if (frontendEnabled) {
    Yon.setFrontendRequestHandler((request) => serveStaticPreviewRequest(distPath, request, { allowRootFallback: false }));
}
else {
    Yon.setFrontendRequestHandler(null);
}
/** @type {ReturnType<typeof setTimeout> | undefined} */
let debounceTimer;
/** @param {string} hostname */
function isLoopbackHost(hostname) {
    return hostname === '127.0.0.1' || hostname === 'localhost' || hostname === '::1';
}
/**
 * @param {Request} req
 * @param {string} hostname
 */
function isAuthorizedHmrRequest(req, hostname) {
    if (isLoopbackHost(hostname))
        return true;
    const token = process.env.HMR_TOKEN || process.env.DEV_TOKEN;
    if (!token)
        return false;
    const url = new URL(req.url);
    const provided = req.headers.get('X-Tachyon-Dev-Token') || url.searchParams.get('token');
    return provided === token;
}
/** @param {BunServer} server */
async function startHmrWatchers(server) {
    if (hmrWatchersStarted)
        return;
    hmrWatchersStarted = true;
    const onFileChange = () => {
        clearTimeout(debounceTimer);
        debounceTimer = setTimeout(async () => {
            try {
                serveLogger.info('HMR reload started');
                await configureRoutes(true);
                server.reload({ routes: Router.reqRoutes });
                for (const client of hmrClients)
                    client.enqueue("event: reload\ndata: reload\n\n");
            }
            catch (err) {
                serveLogger.error('HMR reload failed', { err });
            }
        }, HMR_DEBOUNCE_MS);
    };
    if (await pathExists(Router.routesPath)) {
        hmrWatchers.push(watch(Router.routesPath, { recursive: true }, onFileChange));
    }
    if (Router.pagesPath !== Router.routesPath && await pathExists(Router.pagesPath)) {
        hmrWatchers.push(watch(Router.pagesPath, { recursive: true }, onFileChange));
    }
    if (await pathExists(Router.componentsPath)) {
        hmrWatchers.push(watch(Router.componentsPath, { recursive: true }, onFileChange));
    }
    if (await pathExists(Router.assetsPath)) {
        hmrWatchers.push(watch(Router.assetsPath, { recursive: true }, onFileChange));
    }
    if (await pathExists(Router.sharedDataPath)) {
        hmrWatchers.push(watch(Router.sharedDataPath, { recursive: true }, onFileChange));
    }
    if (await pathExists(Router.sharedScriptsPath)) {
        hmrWatchers.push(watch(Router.sharedScriptsPath, { recursive: true }, onFileChange));
    }
    if (await pathExists(Router.sharedStylesPath)) {
        hmrWatchers.push(watch(Router.sharedStylesPath, { recursive: true }, onFileChange));
    }
}
const server = Bun.serve({
    idleTimeout: process.env.TIMEOUT ? Number(process.env.TIMEOUT) : 0,
    fetch(req, server) {
        const pathname = new URL(req.url).pathname;
        if (hmrEnabled && pathname === "/hmr") {
            if (!isAuthorizedHmrRequest(req, server.hostname ?? '127.0.0.1')) {
                return new Response("Forbidden", { status: 403 });
            }
            if (hmrClients.size >= hmrMaxClients) {
                return new Response("Too Many HMR Clients", { status: 429 });
            }
            server.timeout(req, 0);
            /** @type {HmrClient | null} */
            let hmrController = null;
            return new Response(new ReadableStream({
                async start(controller) {
                    hmrController = controller;
                    hmrClients.add(controller);
                    controller.enqueue(": connected\n\n");
                    await startHmrWatchers(server);
                },
                cancel() {
                    if (hmrController)
                        hmrClients.delete(hmrController);
                },
            }), {
                headers: {
                    "Content-Type": "text/event-stream",
                    "Cache-Control": "no-cache",
                    "Connection": "keep-alive",
                }
            });
        }
        if (hmrEnabled
            && pathname === "/hot-reload-client.js"
            && (req.method === 'GET' || req.method === 'HEAD')) {
            return new Response(req.method === 'HEAD' ? null : Bun.file(hotReloadClientPath), {
                headers: {
                    "Content-Type": "text/javascript; charset=utf-8",
                    "Cache-Control": "no-cache, must-revalidate",
                },
            });
        }
        if (frontendEnabled) {
            const allowRootFallback = wantsHtmlDocument(req) && !isAssetRequest(pathname);
            return serveStaticPreviewRequest(distPath, req, { allowRootFallback })
                .then((response) => response ?? new Response("Not Found", { status: 404 }));
        }
        return new Response("Not Found", { status: 404 });
    },
    routes: Router.reqRoutes,
    port: process.env.PORT || 8080,
    hostname: process.env.HOST || process.env.HOSTNAME || '127.0.0.1',
    development: !!process.env.DEV,
});
serveLogger.info('Server started', {
    url: `http://${server.hostname}:${server.port}`,
    startupMs: Date.now() - start,
    bundleWatchEnabled,
    hmrEnabled,
    appMode: appShape.mode,
    frontendEnabled,
    backendEnabled,
});
process.on('SIGINT', () => {
    clearTimeout(debounceTimer);
    for (const watcher of hmrWatchers)
        watcher.close();
    Pool.clearWarmedProcesses();
    bundleWatcher?.kill();
    server.stop();
    serveLogger.info('Server stopped', { signal: 'SIGINT' });
    process.exit(0);
});
process.on('SIGTERM', () => {
    clearTimeout(debounceTimer);
    for (const watcher of hmrWatchers)
        watcher.close();
    Pool.clearWarmedProcesses();
    bundleWatcher?.kill();
    server.stop();
    serveLogger.info('Server stopped', { signal: 'SIGTERM' });
    process.exit(0);
});
