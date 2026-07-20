#!/usr/bin/env bun
// @ts-check
import path from 'path';
import { access, readFile, rename, rm } from 'fs/promises';
import { pathToFileURL } from 'url';
import { generateNativeHost } from '../compiler/native/index.js';
import { resolveNativeAppConfig } from '../compiler/native/config.js';
import { isNativeTarget, readTargetArg, resolveSingleBundleTarget } from '../shared/native-targets.js';
import { readRenderModeArg, resolveRenderMode } from '../shared/render-mode.js';
import logger from '../server/observability/logger.js';

const rawTarget = readTargetArg(process.argv) ?? process.env.TAC_BUNDLE_TARGET ?? process.env.TAC_TARGET ?? null;
if (!rawTarget) {
    console.error('Usage: tac.native-bundle --target <macos|windows|linux|ios|android>');
    process.exit(1);
}

let target = 'web';
try {
    target = resolveSingleBundleTarget(rawTarget);
}
catch {
    console.error(`Unsupported native target '${rawTarget}'. Supported: macos, windows, linux, ios, android.`);
    process.exit(1);
}
if (!isNativeTarget(target)) {
    console.error(`Unsupported native target '${rawTarget}'. Supported: macos, windows, linux, ios, android.`);
    process.exit(1);
}

const distPath = path.resolve(process.env.YON_DIST_PATH ?? path.join(process.cwd(), 'dist'));
const assetRoot = path.join(distPath, target);
const outputRoot = path.join(distPath, `${target}-native`);
const renderMode = resolveRenderMode(target, readRenderModeArg(process.argv) ?? process.env.TAC_RENDER_MODE);

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

const nativeLogger = logger.child({ scope: 'cli:native-bundle' });

async function main() {
    const appName = await resolveAppName();
    const nativeConfig = await resolveNativeAppConfig();
    /** @type {Record<string, any>} */
    let tacConfig = {};
    const configPath = path.resolve(process.cwd(), 'tac.config.js');
    let hasTacConfig = true;
    try {
        await access(configPath);
    }
    catch (error) {
        if (/** @type {NodeJS.ErrnoException} */ (error).code === 'ENOENT') hasTacConfig = false;
        else throw error;
    }
    if (hasTacConfig) {
        const module = await import(`${pathToFileURL(configPath).href}?t=${Date.now()}`);
        tacConfig = module.default && typeof module.default === 'object' ? module.default : {};
    }
    nativeLogger.info('Generating native host', { target, assetRoot, outputRoot: assetRoot });
    await generateNativeHost({
        target,
        assetRoot,
        outputRoot,
        appName,
        ...nativeConfig,
        nativeHostExtensions: Array.isArray(tacConfig.nativeHostExtensions) ? tacConfig.nativeHostExtensions : [],
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
    // Native targets ship at dist/<target>/. Replace the web-bundle assets with
    // the generated host (its assets are embedded under Resources/).
    await rm(assetRoot, { recursive: true, force: true });
    await rename(outputRoot, assetRoot);
    nativeLogger.info('Native host generated', { target, outputRoot: assetRoot });
}

main().catch((error) => {
    nativeLogger.error('Native host generation failed', { target, err: error });
    console.error(error);
    process.exit(1);
});
