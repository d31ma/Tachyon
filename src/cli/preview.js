#!/usr/bin/env bun
// @ts-check
import { access, readdir, stat } from 'fs/promises';
import path from 'path';
import Router from '../server/route-handler.js';
import Tac from '../compiler/template-compiler.js';
import { createStaticPreviewServer, resolvePreviewFile } from '../runtime/static-preview.js';
import logger from '../server/logger.js';
const distPath = path.join(process.cwd(), 'dist');
const watchMode = process.argv.includes('--watch') || process.argv.includes('--bundle-watch');
const WATCH_INTERVAL_MS = 300;
const bundleCliPath = path.join(import.meta.dir, 'bundle.js');
const previewLogger = logger.child({ scope: 'cli:preview' });
/**
 * @param {string} targetPath
 * @returns {Promise<string[]>}
 */
async function pathFingerprint(targetPath) {
    try {
        const info = await stat(targetPath);
        if (info.isFile()) {
            return [`${targetPath}:${info.size}:${info.mtimeMs}`];
        }
        if (!info.isDirectory())
            return [];
    }
    catch {
        return [];
    }
    const entries = await readdir(targetPath, { withFileTypes: true });
    const fingerprints = [];
    for (const entry of entries) {
        fingerprints.push(...await pathFingerprint(path.join(targetPath, entry.name)));
    }
    return fingerprints;
}
async function buildFingerprint() {
    const entries = await Promise.all([
        pathFingerprint(Router.pagesPath),
        pathFingerprint(Router.routesPath),
        pathFingerprint(Router.componentsPath),
        pathFingerprint(Router.assetsPath),
        pathFingerprint(Router.sharedDataPath),
        pathFingerprint(Router.sharedScriptsPath),
        pathFingerprint(Router.sharedStylesPath),
        ...Tac.getMainEntryCandidates().map((candidate) => pathFingerprint(candidate)),
        pathFingerprint(path.join(process.cwd(), 'package.json'))
    ]);
    return entries.flat().sort().join('|');
}
async function runFreshBundleBuild() {
    const proc = Bun.spawn(['bun', bundleCliPath], {
        cwd: process.cwd(),
        env: process.env,
        stdout: 'inherit',
        stderr: 'inherit'
    });
    const exitCode = await proc.exited;
    if (exitCode !== 0) {
        throw new Error(`Bundle subprocess exited with code ${exitCode}`);
    }
}
async function startPreviewBundleWatcher() {
    await runFreshBundleBuild();
    let lastFingerprint = await buildFingerprint();
    let building = false;
    let queued = false;
    const runQueuedBuild = async () => {
        if (building) {
            queued = true;
            return;
        }
        building = true;
        try {
            await runFreshBundleBuild();
            lastFingerprint = await buildFingerprint();
        }
        catch (err) {
            previewLogger.error('Preview rebuild failed', { err });
        }
        finally {
            building = false;
            if (queued) {
                queued = false;
                await runQueuedBuild();
            }
        }
    };
    const timer = setInterval(async () => {
        const nextFingerprint = await buildFingerprint();
        if (nextFingerprint === lastFingerprint)
            return;
        await runQueuedBuild();
    }, WATCH_INTERVAL_MS);
    return {
        close() {
            clearInterval(timer);
        }
    };
}
let bundleWatcher = null;
if (watchMode) {
    bundleWatcher = await startPreviewBundleWatcher();
}
else {
    try {
        await access(distPath);
    }
    catch {
        previewLogger.error('Missing dist directory', {
            distPath,
            suggestion: "Run 'tac.bundle' first.",
        });
        process.exit(1);
    }
}
const rootPreviewFile = await resolvePreviewFile(distPath, '/');
if (!rootPreviewFile) {
    previewLogger.warn('No previewable root page found', {
        distPath,
        expectedRoot: path.join(distPath, 'index.html'),
        suggestion: 'Run preview from the app directory that contains your client pages before opening /.'
    });
}
const start = Date.now();
const server = await createStaticPreviewServer(distPath);
previewLogger.info('Preview server started', {
    url: `http://${server.hostname}:${server.port}`,
    startupMs: Date.now() - start,
    watchMode,
});
for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, () => {
        bundleWatcher?.close();
        server.stop();
        process.exit(0);
    });
}
