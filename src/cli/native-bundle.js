#!/usr/bin/env bun
// @ts-check
import path from 'path';
import { readFile, rename, rm } from 'fs/promises';
import { generateNativeHost } from '../compiler/native/index.js';
import { resolveNativeAppConfig } from '../compiler/native/config.js';
import { isNativeTarget, readTargetArg, resolveSingleBundleTarget } from '../shared/native-targets.js';
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
    nativeLogger.info('Generating native host', { target, assetRoot, outputRoot: assetRoot });
    await generateNativeHost({ target, assetRoot, outputRoot, appName, ...nativeConfig });
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
