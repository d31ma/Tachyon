// @ts-check
/**
 * Native host orchestrator.
 *
 * Maps a target name to its platform generator and drives generation of the
 * buildable native project.
 */

import path from 'path';
import { access } from 'fs/promises';
import { isNativeTarget } from '../../shared/native-targets.js';
import PlatformGenerator from './platform-generator.js';
import MacOSGenerator from './platforms/macos.js';
import WindowsGenerator from './platforms/windows.js';
import LinuxGenerator from './platforms/linux.js';
import IOSGenerator from './platforms/ios.js';
import AndroidGenerator from './platforms/android.js';

/**
 * @typedef {typeof import('./platform-generator.js').default} PlatformGeneratorClass
 * @typedef {object} GenerateOptions
 * @property {string} target
 * @property {string} assetRoot
 * @property {string} outputRoot
 * @property {string} appName
 * @property {string} [appId]
 * @property {string} [version]
 * @property {string[]} [devicePermissions]
 * @property {string[]} [nativeCapabilities]
 */

/** @type {Record<string, PlatformGeneratorClass>} */
const GENERATORS = {
    macos: MacOSGenerator,
    windows: WindowsGenerator,
    linux: LinuxGenerator,
    ios: IOSGenerator,
    android: AndroidGenerator,
};

/**
 * @param {string} target
 * @returns {target is keyof typeof GENERATORS}
 */
function isRegisteredTarget(target) {
    return isNativeTarget(target) && Object.hasOwn(GENERATORS, target);
}

/**
 * @param {GenerateOptions} options
 * @returns {Promise<void>}
 */
export async function generateNativeHost(options) {
    const Generator = GENERATORS[options.target];
    if (!Generator || !isRegisteredTarget(options.target)) {
        throw new Error(`No native host generator registered for target '${options.target}'. Supported targets: ${Object.keys(GENERATORS).join(', ')}`);
    }

    const assetRoot = path.resolve(options.assetRoot);
    const outputRoot = path.resolve(options.outputRoot);
    if (assetRoot === outputRoot) {
        throw new Error(`Native host output root must be separate from asset root: ${assetRoot}`);
    }
    try {
        await access(assetRoot);
    }
    catch {
        throw new Error(`Native host asset root does not exist: ${assetRoot}`);
    }

    const generator = new Generator({
        target: options.target,
        assetRoot,
        outputRoot,
        appName: options.appName,
        appId: options.appId,
        version: options.version,
        devicePermissions: options.devicePermissions,
        nativeCapabilities: options.nativeCapabilities,
    });

    await generator.generate();
}

export { PlatformGenerator, MacOSGenerator, WindowsGenerator, LinuxGenerator, IOSGenerator, AndroidGenerator };
