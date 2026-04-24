#!/usr/bin/env bun
// @ts-check
import Router from "../server/route-handler.js";
import Tac from "../compiler/template-compiler.js";
import logger from "../server/logger.js";
import { access, mkdir, readdir, rm, stat } from "fs/promises";
import { watch } from "fs";
import path from "path";

/**
 * @typedef {'main' | 'full' | 'page' | 'component' | 'asset'} BuildChangeType
 * @typedef {'file' | 'directory'} WatchTargetKind
 * @typedef {{ type: BuildChangeType, relative: string }} BuildChange
 * @typedef {{ incremental?: boolean }} BundleWatcherOptions
 */

const distPath = `${process.cwd()}/dist`;
const watchMode = process.argv.includes('--watch');
const skipInitialBuild = process.argv.includes('--skip-initial-build');
const WATCH_DEBOUNCE_MS = 200;
const BUILD_CONCURRENCY = 8;
const bundleLogger = logger.child({ scope: 'cli:bundle' });
/**
 * @template T
 * @param {T[]} items
 * @param {number} limit
 * @param {(item: T) => Promise<void>} worker
 */
async function runWithConcurrency(items, limit, worker) {
    if (items.length === 0)
        return;
    let index = 0;
    const runners = Array.from({ length: Math.min(limit, items.length) }, async () => {
        while (index < items.length) {
            const current = items[index++];
            await worker(current);
        }
    });
    await Promise.all(runners);
}
/** @param {string} route */
async function buildRouteOutput(route) {
    // Skip the HMR script — it is a dev-only asset
    if (route.includes('hot-reload-client'))
        return;
    const handler = Router.reqRoutes[route]?.GET;
    if (!handler)
        return;
    try {
        const res = await handler();
        await Bun.write(Bun.file(`${distPath}${route}`), await res.blob());
    }
    catch (err) {
        bundleLogger.error('Failed to build route', { route, err });
    }
}
export async function runBuild() {
    const start = Date.now();
    Router.resetStaticState();
    await rm(distPath, { recursive: true, force: true });
    await mkdir(distPath, { recursive: true });
    await Tac.createStaticRoutes();
    await runWithConcurrency(Object.keys(Router.reqRoutes), BUILD_CONCURRENCY, buildRouteOutput);
    await Tac.prerenderStaticPages(distPath);
    bundleLogger.info('Bundle completed', {
        durationMs: Date.now() - start,
        routeCount: Object.keys(Router.reqRoutes).length,
        distPath,
    });
}
/** @param {string} route */
async function writeRouteOutput(route) {
    await buildRouteOutput(route);
}
/** @param {string} filePath */
async function pathExists(filePath) {
    try {
        await access(filePath);
        return true;
    }
    catch {
        return false;
    }
}
/** @param {string} filePath */
function normalizeRelative(filePath) {
    return filePath.replaceAll(path.sep, '/').replace(/^\.\//, '');
}
/**
 * @param {string} targetPath
 * @returns {BuildChange}
 */
function classifyChange(targetPath) {
    const relative = normalizeRelative(path.relative(process.cwd(), targetPath));
    if (Tac.isMainEntrypoint(relative))
        return { type: 'main', relative };
    if (relative === 'package.json')
        return { type: 'full', relative };
    const pagesPrefix = normalizeRelative(path.relative(process.cwd(), Router.pagesPath)) + '/';
    const componentsPrefix = normalizeRelative(path.relative(process.cwd(), Router.componentsPath)) + '/';
    const assetsPrefix = normalizeRelative(path.relative(process.cwd(), Router.assetsPath)) + '/';
    const sharedDataPrefix = normalizeRelative(path.relative(process.cwd(), Router.sharedDataPath)) + '/';
    const sharedScriptsPrefix = normalizeRelative(path.relative(process.cwd(), Router.sharedScriptsPath)) + '/';
    const sharedStylesPrefix = normalizeRelative(path.relative(process.cwd(), Router.sharedStylesPath)) + '/';
    if (relative.startsWith(pagesPrefix)) {
        const routeFile = relative.slice(pagesPrefix.length);
        if (routeFile.endsWith('/index.html') || routeFile === 'index.html')
            return { type: 'page', relative: routeFile };
        return { type: 'full', relative };
    }
    if (relative.startsWith(componentsPrefix)) {
        const componentFile = relative.slice(componentsPrefix.length);
        if (componentFile.endsWith('.html'))
            return { type: 'component', relative: componentFile };
        return { type: 'full', relative };
    }
    if (relative.startsWith(assetsPrefix)) {
        return { type: 'asset', relative: relative.slice(assetsPrefix.length) };
    }
    if (relative.startsWith(sharedDataPrefix)) {
        return { type: 'full', relative };
    }
    if (relative.startsWith(sharedScriptsPrefix) || relative.startsWith(sharedStylesPrefix)) {
        return { type: 'full', relative };
    }
    return { type: 'full', relative };
}
/** @param {BuildChange} change */
async function runSelectiveBuild(change) {
    const start = Date.now();
    const logIncrementalBuild = () => bundleLogger.info('Incremental build completed', {
        changeType: change.type,
        target: change.relative,
        durationMs: Date.now() - start,
    });
    if (change.type === 'main') {
        const previousRoutes = new Set(Tac.getBrowserRuntimeRoutes());
        const outputRoutes = await Tac.bundleBrowserRuntimeAssets();
        for (const staleRoute of previousRoutes) {
            if (!outputRoutes.includes(staleRoute)) {
                await rm(`${distPath}${staleRoute}`, { force: true });
            }
        }
        for (const route of outputRoutes) {
            await writeRouteOutput(route);
        }
        await Tac.prerenderRoutes(distPath, Tac.getHtmlRoutes());
        logIncrementalBuild();
        return;
    }
    if (change.type === 'asset') {
        const sourcePath = path.join(Router.assetsPath, change.relative);
        const outputPath = path.join(distPath, 'shared', 'assets', change.relative);
        if (!await pathExists(sourcePath)) {
            delete Router.reqRoutes[`/shared/assets/${change.relative}`];
            await rm(outputPath, { force: true });
        }
        else {
            await Tac.bundleAssetFile(change.relative);
            await writeRouteOutput(`/shared/assets/${change.relative}`);
        }
        logIncrementalBuild();
        return;
    }
    if (change.type === 'component') {
        const sourcePath = path.join(Router.componentsPath, change.relative);
        const outputRoute = `/components/${change.relative.replace('.html', '.js')}`;
        if (!await pathExists(sourcePath)) {
            delete Router.reqRoutes[outputRoute];
            await rm(`${distPath}${outputRoute}`, { force: true });
            await runBuild();
            return;
        }
        await Tac.bundleComponentFile(change.relative);
        await writeRouteOutput(outputRoute);
        await Tac.prerenderRoutes(distPath, Tac.getHtmlRoutes());
        logIncrementalBuild();
        return;
    }
    if (change.type === 'page') {
        const sourcePath = path.join(Router.pagesPath, change.relative);
        const outputRoute = `/pages/${change.relative.replace(/\.html$/, '.js')}`;
        const routePath = Tac.routePathFromPageSource(change.relative);
        if (!await pathExists(sourcePath)) {
            delete Router.reqRoutes[outputRoute];
            await rm(`${distPath}${outputRoute}`, { force: true });
            await rm(routePath === '/' ? `${distPath}/index.html` : `${distPath}${routePath}/index.html`, { force: true });
            await runBuild();
            return;
        }
        await Tac.bundlePageFile(change.relative);
        await writeRouteOutput(outputRoute);
        await Tac.prerenderRoutes(distPath, Tac.getHtmlRoutes());
        logIncrementalBuild();
        return;
    }
    await runBuild();
}
/**
 * @param {string} root
 * @param {string[]} [results]
 * @returns {Promise<string[]>}
 */
async function collectDirectories(root, results = []) {
    try {
        const info = await stat(root);
        if (!info.isDirectory())
            return results;
    }
    catch {
        return results;
    }
    results.push(root);
    for (const entry of await readdir(root, { withFileTypes: true })) {
        if (entry.isDirectory()) {
            await collectDirectories(path.join(root, entry.name), results);
        }
    }
    return results;
}
/**
 * @param {(targetPath: string, eventType: string) => void} onChange
 */
async function watchPaths(onChange) {
    /** @type {Map<string, import('fs').FSWatcher>} */
    const watchers = new Map();
    const roots = [
        Router.pagesPath,
        Router.componentsPath,
        Router.assetsPath,
        Router.sharedDataPath,
        Router.sharedScriptsPath,
        Router.sharedStylesPath,
    ];
    /**
     * @param {string} targetPath
     * @param {WatchTargetKind} kind
     */
    const addWatcher = (targetPath, kind) => {
        if (watchers.has(targetPath))
            return;
        try {
            const watcher = watch(targetPath, { persistent: true }, (eventType, filename) => {
                const changedPath = filename && kind === 'directory'
                    ? path.resolve(targetPath, filename.toString())
                    : targetPath;
                onChange(changedPath, eventType);
            });
            watchers.set(targetPath, watcher);
        }
        catch {
            // ignore paths the platform cannot watch directly
        }
    };
    const syncDirectoryWatchers = async () => {
        const active = new Set();
        for (const root of roots) {
            for (const dir of await collectDirectories(root)) {
                active.add(dir);
                addWatcher(dir, 'directory');
            }
        }
        for (const file of [...Tac.getMainEntryCandidates(), `${process.cwd()}/package.json`]) {
            active.add(file);
            addWatcher(file, 'file');
        }
        for (const [target, watcher] of watchers.entries()) {
            if (!active.has(target)) {
                watcher.close();
                watchers.delete(target);
            }
        }
    };
    await syncDirectoryWatchers();
    return {
        async refresh() {
            await syncDirectoryWatchers();
        },
        close() {
            for (const watcher of watchers.values())
                watcher.close();
            watchers.clear();
        }
    };
}
/**
 * @param {BundleWatcherOptions} [options]
 */
export async function startBundleWatcher(options = {}) {
    const incremental = options.incremental ?? true;
    if (!skipInitialBuild) {
        await runBuild();
    }
    /** @type {ReturnType<typeof setTimeout> | undefined} */
    let debounceTimer;
    let building = false;
    let queued = false;
    /** @type {BuildChange | null} */
    let pendingChange = null;
    /**
     * @param {BuildChange | null} current
     * @param {BuildChange} next
     * @returns {BuildChange}
     */
    const mergeChanges = (current, next) => {
        if (!current)
            return next;
        if (current.type === 'full' || next.type === 'full')
            return { type: 'full', relative: next.relative };
        if (current.type === next.type && current.relative === next.relative)
            return current;
        return { type: 'full', relative: next.relative };
    };
    /**
     * @param {string} targetPath
     * @param {string} eventType
     */
    const schedule = (targetPath, eventType) => {
        clearTimeout(debounceTimer);
        /** @type {BuildChange} */
        const nextChange = eventType === 'rename'
            ? { type: 'full', relative: normalizeRelative(path.relative(process.cwd(), targetPath)) }
            : classifyChange(targetPath);
        pendingChange = mergeChanges(pendingChange, nextChange);
        debounceTimer = setTimeout(async () => {
            if (building) {
                queued = true;
                return;
            }
            building = true;
            try {
                await watcher.refresh();
                const change = pendingChange ?? { type: 'full', relative: 'unknown' };
                pendingChange = null;
                if (incremental) {
                    await runSelectiveBuild(change);
                }
                else {
                    await runBuild();
                }
            }
            catch (err) {
                bundleLogger.error('Watch rebuild failed', {
                    changedPath: targetPath,
                    eventType,
                    err,
                });
            }
            finally {
                building = false;
                if (queued) {
                    queued = false;
                    pendingChange = mergeChanges(pendingChange, { type: 'full', relative: 'queued-change' });
                    schedule(process.cwd(), 'change');
                }
            }
        }, WATCH_DEBOUNCE_MS);
    };
    const watcher = await watchPaths(schedule);
    bundleLogger.info('Watching source paths for bundle changes', {
        pagesPath: Router.pagesPath,
        componentsPath: Router.componentsPath,
        assetsPath: Router.assetsPath,
        sharedDataPath: Router.sharedDataPath,
        sharedScriptsPath: Router.sharedScriptsPath,
        sharedStylesPath: Router.sharedStylesPath,
    });
    return {
        close() {
            clearTimeout(debounceTimer);
            watcher.close();
        }
    };
}
if (import.meta.main) {
    if (!watchMode) {
        await runBuild();
    }
    else {
        const bundleWatcher = await startBundleWatcher();
        const shutdown = () => {
            bundleWatcher.close();
            process.exit(0);
        };
        process.on('SIGINT', shutdown);
        process.on('SIGTERM', shutdown);
        await new Promise(() => { });
    }
}
