#!/usr/bin/env bun
// @ts-check
import { access, readdir, stat } from 'fs/promises';
import path from 'path';
import Router from '../server/http/route-handler.js';
import Compiler from '../compiler/index.js';
import { createStaticPreviewServer, resolvePreviewFile, resolveStaticPreviewRoot } from '../runtime/static-preview.js';
import { isNativeTarget, readTargetArg, resolveSingleBundleTarget } from '../shared/native-targets.js';
import { checkNativePreviewRequirements, formatNativePreviewCheckFailure } from '../shared/native-preview-checks.js';
import logger from '../server/observability/logger.js';
const distPath = path.join(process.cwd(), 'dist');
const watchMode = process.argv.includes('--watch') || process.argv.includes('--bundle-watch');
const skipNativeChecks = process.argv.includes('--skip-native-checks');
let previewTarget = 'web';
try {
    previewTarget = resolveSingleBundleTarget(readTargetArg(process.argv) ?? process.env.TAC_PREVIEW_TARGET ?? process.env.TAC_TARGET ?? 'web');
}
catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
}
const WATCH_INTERVAL_MS = 300;
const bundleCliPath = path.join(import.meta.dir, 'bundle.js');
const previewLogger = logger.child({ scope: 'cli:preview' });

/**
 * @param {string[]} argv
 * @param {string} longName
 * @returns {string | null}
 */
function readStringArg(argv, longName) {
    for (let index = 0; index < argv.length; index += 1) {
        const arg = argv[index];
        if (arg === longName) {
            return argv[index + 1] ?? null;
        }
        if (arg.startsWith(`${longName}=`)) {
            return arg.slice(longName.length + 1);
        }
    }
    return null;
}

/**
 * @param {string[]} argv
 * @returns {{ port?: number, hostname?: string }}
 */
function readPreviewServerOptions(argv) {
    const options = /** @type {{ port?: number, hostname?: string }} */ ({});
    const portArg = readStringArg(argv, '--port');
    if (portArg !== null) {
        const port = Number(portArg);
        if (!Number.isInteger(port) || port < 0 || port > 65535) {
            throw new Error(`Invalid preview port '${portArg}'. Expected an integer from 0 to 65535.`);
        }
        options.port = port;
    }
    const hostArg = readStringArg(argv, '--host') ?? readStringArg(argv, '--hostname');
    if (hostArg !== null) {
        const hostname = hostArg.trim();
        if (!hostname) {
            throw new Error('Invalid preview host. Expected a non-empty hostname.');
        }
        options.hostname = hostname;
    }
    return options;
}
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
        ...Compiler.getMainEntryCandidates().map((candidate) => pathFingerprint(candidate)),
        pathFingerprint(path.join(process.cwd(), 'package.json'))
    ]);
    return entries.flat().sort().join('|');
}
async function runFreshBundleBuild() {
    const proc = Bun.spawn(['bun', bundleCliPath, '--target', previewTarget], {
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
        catch (error) {
            previewLogger.error('Preview rebuild failed', { err: error });
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
if (isNativeTarget(previewTarget) && !skipNativeChecks) {
    const check = await checkNativePreviewRequirements(previewTarget);
    if (!check.ok) {
        previewLogger.error('Native preview prerequisites failed', {
            target: previewTarget,
            missing: check.missing,
        });
        console.error(formatNativePreviewCheckFailure(previewTarget, check));
        process.exit(1);
    }
}
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
const previewRoot = await resolveStaticPreviewRoot(distPath, previewTarget);
const rootPreviewFile = await resolvePreviewFile(previewRoot, '/');
if (!rootPreviewFile) {
    previewLogger.warn('No previewable root page found', {
        distPath: previewRoot,
        expectedRoot: path.join(previewRoot, 'index.html'),
        suggestion: 'Run preview from the app directory that contains your client pages before opening /.'
    });
}
const start = Date.now();
let previewOptions;
try {
    previewOptions = readPreviewServerOptions(process.argv);
}
catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
}
const server = await createStaticPreviewServer(previewRoot, previewOptions);
previewLogger.info(`Preview ready in ${Date.now() - start}ms → http://${server.hostname}:${server.port}`, {
    target: previewTarget,
    watch: watchMode,
});
for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, () => {
        bundleWatcher?.close();
        server.stop();
        process.exit(0);
    });
}
