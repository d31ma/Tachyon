#!/usr/bin/env bun
// @ts-check
import Router from "../server/http/route-handler.js";
import Compiler from "../compiler/index.js";
import { generateNativeHost } from "../compiler/native/index.js";
import { resolveNativeAppConfig } from "../compiler/native/config.js";
import { resolveNativeHostExtensions } from "../compiler/native/extensions.js";
import { hasNativePackager, packageNativeArtifact } from "../compiler/native/packagers.js";
import { NATIVE_TARGET_SET, readTargetArg, resolveBundleTargets } from "../shared/native-targets.js";
import { readRenderModeArg, resolveRenderModes } from "../shared/render-mode.js";
import NativeUIBundleCompiler from "../compiler/native-ui/bundle-compiler.js";
import logger from "../server/observability/logger.js";
import { access, chmod, mkdir, mkdtemp, readFile, readdir, rename, rm, stat } from "fs/promises";
import { watch } from "fs";
import path from "path";
import { pathToFileURL } from "url";

/**
 * @typedef {'file' | 'directory'} WatchTargetKind
 * @typedef {{ incremental?: boolean, skipInitialBuild?: boolean }} BundleWatcherOptions
 * @typedef {import("../shared/native-targets.js").BundleTarget} BundleTarget
 */

const distPath = path.resolve(process.env.YON_DIST_PATH ?? path.join(process.cwd(), 'dist'));
let activeDistPath = distPath;
const watchMode = process.argv.includes('--watch');
const skipInitialBuild = process.argv.includes('--skip-initial-build');
const skipNativeHost = process.argv.includes('--skip-native-host');
// Fail the build when generated output still contains constructs a strict CSP
// blocks without 'unsafe-eval' (#109). Also on via TAC_CSP_CHECK=1.
const cspCheck = process.argv.includes('--csp-check')
    || process.env.TAC_CSP_CHECK === '1' || process.env.TAC_CSP_CHECK === 'true';
// Artifact export (.apk / .ipa): on for production bundles, opt-in elsewhere
// with --package, opt-out with --skip-package. Missing toolchains downgrade
// to a logged skip — the generated host project is always usable by hand.
const packageNativeArtifacts = !process.argv.includes('--skip-package')
    && (process.argv.includes('--package') || process.env.NODE_ENV === 'production');
const bundleTargets = resolveBundleTargets(readTargetArg(process.argv) ?? process.env.TAC_BUNDLE_TARGET ?? process.env.TAC_TARGET);
const renderModes = resolveRenderModes(
    bundleTargets,
    readRenderModeArg(process.argv) ?? process.env.TAC_RENDER_MODE,
);
process.env.TAC_BUNDLE_TARGETS = bundleTargets.join(',');
const WATCH_DEBOUNCE_MS = 200;
const BUILD_CONCURRENCY = 8;
const bundleLogger = logger.child({ scope: 'cli:bundle' });

/**
 * Opt-in phase timing for the build harness. When TAC_BENCH is set, records how
 * long each build phase took into globalThis.__TAC_BENCH__ so a caller can read
 * the compile-vs-write-vs-prerender split. A no-op (just runs the phase) otherwise.
 * @template T @param {string} phase @param {() => Promise<T>} run @returns {Promise<T>}
 */
async function benchPhase(phase, run) {
    if (!process.env.TAC_BENCH)
        return run();
    const started = performance.now();
    try {
        return await run();
    }
    finally {
        (/** @type {any} */ (globalThis).__TAC_BENCH__ ??= []).push({ phase, ms: performance.now() - started });
    }
}
async function resolveAppName() {
    try {
        const pkg = JSON.parse(await readFile(path.join(process.cwd(), 'package.json'), 'utf8'));
        const configuredName = typeof pkg.tachyon?.appName === 'string'
            ? pkg.tachyon.appName.trim()
            : typeof pkg.tac?.appName === 'string'
                ? pkg.tac.appName.trim()
                : typeof pkg.displayName === 'string'
                    ? pkg.displayName.trim()
                    : typeof pkg.productName === 'string'
                        ? pkg.productName.trim()
                        : '';
        if (configuredName)
            return configuredName;
        const name = typeof pkg.name === 'string' ? pkg.name.split('/').pop() : '';
        if (name)
            return name;
    }
    catch { }
    return path.basename(process.cwd());
}
/**
 * Replace a generated host project directory with just its built artifacts.
 * @param {string} projectRoot
 * @param {string[]} artifactPaths
 * @returns {Promise<string[]>} The artifact basenames now under projectRoot.
 */
async function reduceToArtifacts(projectRoot, artifactPaths) {
    const holding = `${projectRoot}-artifacts`;
    await rm(holding, { recursive: true, force: true });
    await mkdir(holding, { recursive: true });
    const names = [];
    for (const artifact of artifactPaths) {
        const name = path.basename(artifact);
        await rename(artifact, path.join(holding, name));
        names.push(name);
    }
    await rm(projectRoot, { recursive: true, force: true });
    await rename(holding, projectRoot);
    return names;
}
async function resolveAppVersion() {
    try {
        const pkg = JSON.parse(await readFile(path.join(process.cwd(), 'package.json'), 'utf8'));
        if (typeof pkg.version === 'string' && pkg.version.trim())
            return pkg.version.trim();
    }
    catch { }
    return '1.0.0';
}
/** @type {Promise<Record<string, any>> | null} */
let tacConfigPromise = null;
async function loadTacConfig() {
    if (tacConfigPromise) return tacConfigPromise;
    const configPath = path.resolve(process.cwd(), 'tac.config.js');
    tacConfigPromise = (async () => {
        try { await access(configPath); }
        catch (error) {
            if (/** @type {NodeJS.ErrnoException} */ (error).code === 'ENOENT') return {};
            throw error;
        }
        const config = (await import(`${pathToFileURL(configPath).href}?t=${Date.now()}`)).default;
        return config && typeof config === 'object' ? config : {};
    })();
    return tacConfigPromise;
}
/** @param {string} distRoot */
async function runPostBundleHook(distRoot = activeDistPath) {
    const hook = (await loadTacConfig()).postBundle;
    if (!hook)
        return;
    await hook({
        distRoot,
        targets: bundleTargets,
        targetRoots: Object.fromEntries(bundleTargets.map((target) => [target, path.join(distRoot, target)])),
    });
}
/**
 * Fails the build when any generated bundle still contains a construct a strict
 * Content-Security-Policy blocks without `'unsafe-eval'`. Enabled by
 * `--csp-check` / `TAC_CSP_CHECK` (#109).
 * @param {string} root Staged output root to audit before the live swap.
 */
async function assertCspSafeOutput(root) {
    const findings = await Compiler.auditCspSafety(root);
    if (findings.length === 0) {
        bundleLogger.info('CSP check passed: no unsafe-eval constructs in generated output');
        return;
    }
    for (const { file, construct, count } of findings)
        bundleLogger.error('CSP unsafe-eval construct in generated output', { file, construct, count });
    const total = findings.reduce((sum, finding) => sum + finding.count, 0);
    throw new Error(`CSP check failed: ${total} unsafe-eval construct(s) across ${findings.length} location(s). A strict "script-src 'self'" cannot load this bundle.`);
}
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
        const response = await handler();
        const outputPath = `${activeDistPath}${route}`;
        await Bun.write(Bun.file(outputPath), await response.blob());
    }
    catch (error) {
        bundleLogger.error('Failed to build route', { route, err: error });
    }
}
export async function runBuild() {
    const start = Date.now();
    const stagingPath = await mkdtemp(path.join(path.dirname(distPath), `${path.basename(distPath)}-staging-`));
    const previousTargetEnv = process.env.TAC_BUNDLE_TARGETS;
    const previousNativeCapabilitiesEnv = process.env.TAC_NATIVE_CAPABILITIES;
    const previousDevicePermissionsEnv = process.env.TAC_NATIVE_DEVICE_PERMISSIONS;
    const previousExtensionCapabilitiesEnv = process.env.TAC_NATIVE_EXTENSION_CAPABILITIES;
    try {
        let routeCount = 0;
        const appName = await resolveAppName();
        const appVersion = await resolveAppVersion();
        const nativeConfig = await resolveNativeAppConfig();
        const tacConfig = await loadTacConfig();
        const { devicePermissions, nativeCapabilities } = nativeConfig;
        const nativeHostExtensions = Array.isArray(tacConfig.nativeHostExtensions) ? tacConfig.nativeHostExtensions : [];
        const resolvedNativeHostExtensions = await resolveNativeHostExtensions(process.cwd(), nativeHostExtensions);
        process.env.TAC_NATIVE_CAPABILITIES = nativeCapabilities.join(',');
        process.env.TAC_NATIVE_DEVICE_PERMISSIONS = devicePermissions.join(',');
        /** @type {Array<{ target: BundleTarget, outputRoot: string }>} */
        const nativeHosts = [];
        for (const target of bundleTargets) {
            Router.resetStaticState();
            process.env.TAC_BUNDLE_TARGETS = target;
            process.env.TAC_NATIVE_EXTENSION_CAPABILITIES = resolvedNativeHostExtensions
                .flatMap((extension) => extension.operations
                    .filter((operation) => operation.targets.includes(target))
                    .map((operation) => operation.name))
                .join(',');
            activeDistPath = path.join(stagingPath, target);
            await mkdir(activeDistPath, { recursive: true });
            await benchPhase('compile', () => Compiler.createStaticRoutes());
            await benchPhase('write', () => runWithConcurrency(Object.keys(Router.reqRoutes), BUILD_CONCURRENCY, buildRouteOutput));
            await benchPhase('prerender', () => Compiler.prerenderStaticPages(activeDistPath));
            if (renderModes[target] === 'native') {
                await benchPhase('native-ui', () => NativeUIBundleCompiler.compile({
                    distRoot: activeDistPath,
                    routes: Compiler.getHtmlRoutes(),
                    adapters: tacConfig.nativeUIAdapters,
                }));
            }
            if (!skipNativeHost && NATIVE_TARGET_SET.has(target)) {
                nativeHosts.push({
                    target,
                    // Temporary staging dir; the generated host is swapped into
                    // the target slot below so it ships at dist/<target>/.
                    outputRoot: path.join(stagingPath, `${target}-native`),
                });
            }
            routeCount = Math.max(routeCount, Object.keys(Router.reqRoutes).length);
        }
        process.env.TAC_BUNDLE_TARGETS = bundleTargets.join(',');
        activeDistPath = stagingPath;
        await runPostBundleHook(stagingPath);
        for (const { target, outputRoot } of nativeHosts) {
            const targetRoot = path.join(stagingPath, target);
            await generateNativeHost({
                target,
                assetRoot: targetRoot,
                outputRoot,
                appName,
                version: appVersion,
                devicePermissions,
                nativeCapabilities,
                permissionOrigins: nativeConfig.permissionOrigins,
                managedContentOrigins: nativeConfig.managedContentOrigins,
                nativeHostExtensions,
                nativeUIAdapters: tacConfig.nativeUIAdapters,
            });
            if (typeof tacConfig.postNativeHost === 'function') {
                const manifest = JSON.parse(await readFile(path.join(outputRoot, 'tachyon.host.json'), 'utf8'));
                await tacConfig.postNativeHost({
                    target,
                    projectRoot: outputRoot,
                    resourcesRoot: path.join(outputRoot, 'Resources'),
                    manifest,
                });
            }
            // Native targets ship the host at dist/<target>/. Replace the
            // standalone web bundle (already embedded in the host's Resources/)
            // with the generated host.
            await rm(targetRoot, { recursive: true, force: true });
            await rename(outputRoot, targetRoot);
            bundleLogger.info('Native host generated', { target, outputRoot: path.join(distPath, target) });
            if (!packageNativeArtifacts || !hasNativePackager(target))
                continue;
            try {
                const result = await packageNativeArtifact({
                    target,
                    projectRoot: targetRoot,
                    appName,
                    version: appVersion,
                });
                if ('artifactPaths' in result) {
                    // Ship only the artifacts: replace the host project at
                    // dist/<target>/ with the built .apk/.ipa/.app output.
                    const artifactNames = await reduceToArtifacts(targetRoot, result.artifactPaths);
                    bundleLogger.info('Native artifact exported', {
                        target,
                        artifacts: artifactNames.map((name) => path.join(distPath, target, name)).join(','),
                    });
                }
                else {
                    bundleLogger.warn('Native artifact export skipped', { target, reason: result.skipped });
                }
            }
            catch (error) {
                // Missing toolchains downgrade to a skip above; an actual
                // build failure must fail the production bundle.
                bundleLogger.error('Native artifact export failed', { target, err: error });
                throw error;
            }
        }
        // Validate the staged output before replacing any live target. This
        // keeps a failed CSP gate atomic and avoids auditing sibling targets
        // left behind by earlier, unrelated builds.
        if (cspCheck)
            await assertCspSafeOutput(stagingPath);
        // Swap in only what this run built: each staged top-level entry
        // (the dist/<target> dirs, plus anything a postBundle hook wrote at
        // the staging root) replaces its live counterpart individually, so
        // sibling targets from earlier runs survive — building android must
        // not delete dist/web out from under a dev server, or dist/macos out
        // from under a running generated app. Each swap is atomic per entry:
        // retire the live dir, rename the staged one in, then drop the
        // retired copy.
        await mkdir(distPath, { recursive: true });
        for (const entry of await readdir(stagingPath)) {
            const staged = path.join(stagingPath, entry);
            const live = path.join(distPath, entry);
            const retired = path.join(path.dirname(distPath), `${path.basename(distPath)}-retired-${entry}-${Date.now()}`);
            let hasRetired = false;
            if (await pathExists(live)) {
                await rename(live, retired);
                hasRetired = true;
            }
            try {
                await rename(staged, live);
            }
            catch (error) {
                if (hasRetired)
                    await rename(retired, live);
                throw error;
            }
            if (hasRetired)
                await rm(retired, { recursive: true, force: true });
        }
        await rm(stagingPath, { recursive: true, force: true });
        bundleLogger.info(`Bundle completed in ${Date.now() - start}ms`, {
            routes: routeCount,
            targets: bundleTargets.join(','),
            nativeHosts: nativeHosts.length ? nativeHosts.map((host) => host.target).join(',') : undefined,
        });
        bundleLogger.debug('Bundle output path', { distPath });
    }
    catch (error) {
        await rm(stagingPath, { recursive: true, force: true });
        throw error;
    }
    finally {
        activeDistPath = distPath;
        process.env.TAC_BUNDLE_TARGETS = previousTargetEnv ?? bundleTargets.join(',');
        if (previousNativeCapabilitiesEnv === undefined) delete process.env.TAC_NATIVE_CAPABILITIES;
        else process.env.TAC_NATIVE_CAPABILITIES = previousNativeCapabilitiesEnv;
        if (previousDevicePermissionsEnv === undefined) delete process.env.TAC_NATIVE_DEVICE_PERMISSIONS;
        else process.env.TAC_NATIVE_DEVICE_PERMISSIONS = previousDevicePermissionsEnv;
        if (previousExtensionCapabilitiesEnv === undefined) delete process.env.TAC_NATIVE_EXTENSION_CAPABILITIES;
        else process.env.TAC_NATIVE_EXTENSION_CAPABILITIES = previousExtensionCapabilitiesEnv;
    }
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
 * Roots whose files, when changed, can trigger a rebuild. Mirrors the watched paths.
 * @returns {string[]}
 */
function watchedRoots() {
    return [
        Router.pagesPath,
        Router.componentsPath,
        Router.assetsPath,
        Router.sharedDataPath,
        Router.sharedScriptsPath,
        Router.sharedStylesPath,
    ];
}
/**
 * Snapshots mtimes of every watched source file. The rebuild decision diffs two
 * snapshots rather than trusting fs event types, which are unreliable across
 * editors (atomic saves fire rename + temp-file events, not change).
 * @returns {Promise<Map<string, number>>}
 */
async function snapshotSources() {
    /** @type {Map<string, number>} */
    const snapshot = new Map();
    await Promise.all(watchedRoots().map(async (root) => {
        if (!await pathExists(root))
            return;
        for (const relative of new Bun.Glob('**/*').scanSync({ cwd: root })) {
            const absolute = path.resolve(root, relative);
            try {
                snapshot.set(absolute, (await stat(absolute)).mtimeMs);
            }
            catch { /* file vanished mid-scan */ }
        }
    }));
    for (const file of [...Compiler.getMainEntryCandidates(), `${process.cwd()}/package.json`]) {
        try {
            snapshot.set(path.resolve(file), (await stat(file)).mtimeMs);
        }
        catch { /* optional file absent */ }
    }
    return snapshot;
}
/** A page/component module's own entry + companion files (tac.html, tac.ts, tac.css, …). */
const TAC_MODULE_FILE = /^tac\.(html|[cm]?jsx?|[cm]?tsx?|css|scss|sass|less)$/;
/**
 * Maps a changed file under a page/component root to its owning module's tac.html
 * route, or null if it isn't a recognised module file (e.g. an imported helper,
 * which Bun.build inlines into unknown consumers and so requires a full rebuild).
 * @param {string} relative
 * @returns {string | null}
 */
function moduleOwnerRoute(relative) {
    const normalized = normalizeRelative(relative);
    const base = normalized.split('/').pop() ?? '';
    if (!TAC_MODULE_FILE.test(base))
        return null;
    const dir = normalized.includes('/') ? normalized.slice(0, normalized.lastIndexOf('/') + 1) : '';
    return `${dir}${Compiler.pageFileName}`;
}
/**
 * @typedef {{ kind: 'page' | 'component' | 'asset', route: string } | { kind: 'other' }} ChangedSource
 * @param {string} absolutePath
 * @returns {ChangedSource}
 */
function classifyPath(absolutePath) {
    const pageRelative = path.relative(Router.pagesPath, absolutePath);
    if (!pageRelative.startsWith('..') && !path.isAbsolute(pageRelative)) {
        const route = moduleOwnerRoute(pageRelative);
        return route ? { kind: 'page', route } : { kind: 'other' };
    }
    const componentRelative = path.relative(Router.componentsPath, absolutePath);
    if (!componentRelative.startsWith('..') && !path.isAbsolute(componentRelative)) {
        const route = moduleOwnerRoute(componentRelative);
        return route ? { kind: 'component', route } : { kind: 'other' };
    }
    const assetRelative = path.relative(Router.assetsPath, absolutePath);
    if (!assetRelative.startsWith('..') && !path.isAbsolute(assetRelative))
        return { kind: 'asset', route: normalizeRelative(assetRelative) };
    return { kind: 'other' };
}
/** Whether in-memory compiler state from a prior full build can back an incremental rebuild. */
let incrementalReady = false;
/**
 * Recompiles a single page and re-prerenders only its route into the live dist.
 * Returns false when this page wraps other pages (it has a slot that nested pages
 * render into, so its markup is baked into their prerendered HTML) — those nested
 * pages would go stale, so the caller falls back to a full rebuild.
 * @param {string} pageRoute page source relative to pagesPath, e.g. 'about/tac.html'
 * @param {string} targetDist dist root for the active target, e.g. dist/web
 * @returns {Promise<boolean>}
 */
async function rebuildPageOutput(pageRoute, targetDist) {
    const publicPath = `/pages/${Compiler.toModuleOutputRoute(pageRoute)}`;
    const routePath = Compiler.routePathFromPageSource(pageRoute);
    const pageWrapsOtherPages = () => Object.values(Compiler.wrapperPages).some((entry) => entry?.path === publicPath);
    const wrappedOtherPagesBefore = pageWrapsOtherPages();
    await Compiler.bundlePageFile(pageRoute);
    if (wrappedOtherPagesBefore || pageWrapsOtherPages())
        return false;
    await buildRouteOutput(publicPath);
    await Compiler.prerenderRoutes(targetDist, [routePath]);
    return true;
}
/** Mtime snapshot of watched sources from the last build; backs the incremental decision. */
let lastSnapshot = new Map();
/**
 * Attempts an incremental rebuild for a set of changed sources. Returns false
 * (caller falls back to full) if any change isn't an isolated page/component/asset
 * edit, or if a changed page wraps other pages.
 * @param {ChangedSource[]} changes
 * @returns {Promise<boolean>}
 */
async function tryIncrementalBuild(changes) {
    if (!incrementalReady || bundleTargets.length !== 1 || changes.length === 0)
        return false;
    if (!changes.every((change) => change.kind !== 'other'))
        return false;
    const target = bundleTargets[0];
    const targetDist = path.join(distPath, target);
    const previousTargetEnv = process.env.TAC_BUNDLE_TARGETS;
    activeDistPath = targetDist;
    process.env.TAC_BUNDLE_TARGETS = target;
    try {
        for (const change of changes) {
            if (change.kind === 'asset') {
                await Compiler.bundleAssetFile(change.route);
                await buildRouteOutput(`/shared/assets/${change.route}`);
            }
            else if (change.kind === 'component') {
                // Components are hydrated client-side from their own module — pages
                // embed only a placeholder — so recompiling the module is sufficient.
                await Compiler.bundleComponentFile(change.route);
                await buildRouteOutput(`/components/${Compiler.toModuleOutputRoute(change.route)}`);
            }
            else if (!await rebuildPageOutput(change.route, targetDist)) {
                return false; // page wraps other pages — full rebuild handles the pages nested in it
            }
        }
        return true;
    }
    finally {
        activeDistPath = distPath;
        process.env.TAC_BUNDLE_TARGETS = previousTargetEnv ?? bundleTargets.join(',');
    }
}
/**
 * Diffs the source tree against the last snapshot and rebuilds the minimal scope.
 * Structural changes (added/removed files) and non-page/asset edits force a full build.
 */
async function reconcileBuild() {
    const start = Date.now();
    const next = await snapshotSources();
    /** @type {string[]} */
    const changedPaths = [];
    let structural = false;
    for (const [absolute, mtime] of next) {
        const previous = lastSnapshot.get(absolute);
        if (previous === undefined)
            structural = true; // new file may add a route
        else if (previous !== mtime)
            changedPaths.push(absolute);
    }
    for (const absolute of lastSnapshot.keys()) {
        if (!next.has(absolute))
            structural = true; // removed file may drop a route
    }
    lastSnapshot = next;
    if (!structural && changedPaths.length === 0)
        return; // spurious event, nothing actually changed
    const changes = [...new Map(changedPaths.map(classifyPath).map((change) => [
        change.kind === 'other' ? Symbol() : `${change.kind}:${change.route}`,
        change,
    ])).values()];
    let mode = 'full';
    if (!structural) {
        try {
            if (await tryIncrementalBuild(changes))
                mode = 'incremental';
        }
        catch (error) {
            bundleLogger.warn('Incremental rebuild failed; falling back to full build', { err: error });
        }
    }
    if (mode === 'full') {
        await runBuild();
        // A single-target full build leaves in-memory state a later incremental can reuse.
        incrementalReady = bundleTargets.length === 1;
        lastSnapshot = await snapshotSources();
    }
    bundleLogger.info(`Rebuilt ${changes.length} file${changes.length === 1 ? '' : 's'} in ${Date.now() - start}ms (${mode})`, {
        structural: structural || undefined,
    });
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
        for (const file of [...Compiler.getMainEntryCandidates(), `${process.cwd()}/package.json`]) {
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
    const shouldSkipInitialBuild = options.skipInitialBuild ?? skipInitialBuild;
    if (!shouldSkipInitialBuild) {
        await runBuild();
        incrementalReady = incremental && bundleTargets.length === 1;
    }
    // Baseline snapshot so the first change diffs to just the edited file(s).
    lastSnapshot = await snapshotSources();
    /** @type {ReturnType<typeof setTimeout> | undefined} */
    let debounceTimer;
    let building = false;
    let queued = false;
    const schedule = () => {
        clearTimeout(debounceTimer);
        debounceTimer = setTimeout(async () => {
            if (building) {
                queued = true;
                return;
            }
            building = true;
            try {
                await watcher.refresh();
                if (incremental) {
                    await reconcileBuild();
                }
                else {
                    await runBuild();
                    lastSnapshot = await snapshotSources();
                }
            }
            catch (error) {
                bundleLogger.error('Watch rebuild failed', { err: error });
            }
            finally {
                building = false;
                if (queued) {
                    queued = false;
                    schedule();
                }
            }
        }, WATCH_DEBOUNCE_MS);
    };
    const watcher = await watchPaths(schedule);
    bundleLogger.info('Watching for changes');
    bundleLogger.debug('Watched source paths', {
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
export async function runBundleCommand() {
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
if (import.meta.main) {
    await runBundleCommand();
}
