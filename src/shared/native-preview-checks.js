// @ts-check
import { access } from 'fs/promises';
import path from 'path';
import { isNativeTarget } from './native-targets.js';

/**
 * @typedef {object} NativePreviewCheckOptions
 * @property {NodeJS.Platform} [platform]
 * @property {NodeJS.ProcessEnv} [env]
 * @property {(command: string, args?: string[]) => Promise<boolean>} [commandAvailable]
 * @property {(filePath: string) => Promise<boolean>} [pathExists]
 */

/**
 * @typedef {object} NativePreviewCheckResult
 * @property {boolean} ok
 * @property {string[]} missing
 * @property {string[]} suggestions
 */

/** @param {string} filePath */
async function defaultPathExists(filePath) {
    try {
        await access(filePath);
        return true;
    }
    catch {
        return false;
    }
}

/**
 * @param {string} command
 * @param {string[]} [args]
 */
async function defaultCommandAvailable(command, args = ['--version']) {
    try {
        const proc = Bun.spawn([command, ...args], {
            stdout: 'ignore',
            stderr: 'ignore',
        });
        const exitCode = await proc.exited;
        return exitCode === 0;
    }
    catch {
        return false;
    }
}

/**
 * @param {NativePreviewCheckResult} result
 * @param {string} missing
 * @param {string} suggestion
 */
function addMissing(result, missing, suggestion) {
    result.ok = false;
    result.missing.push(missing);
    result.suggestions.push(suggestion);
}

/**
 * Verifies the local machine has the minimum prerequisites to preview a native
 * target. `bun preview --target web` intentionally needs no native checks.
 *
 * @param {string} target
 * @param {NativePreviewCheckOptions} [options]
 * @returns {Promise<NativePreviewCheckResult>}
 */
export async function checkNativePreviewRequirements(target, options = {}) {
    const normalized = target.trim().toLowerCase();
    const platform = options.platform ?? process.platform;
    const env = options.env ?? process.env;
    const commandAvailable = options.commandAvailable ?? defaultCommandAvailable;
    const pathExists = options.pathExists ?? defaultPathExists;
    /** @type {NativePreviewCheckResult} */
    const result = { ok: true, missing: [], suggestions: [] };

    if (!isNativeTarget(normalized))
        return result;

    if (normalized === 'macos') {
        if (platform !== 'darwin') {
            addMissing(result, 'macOS host', 'Run macOS native preview on macOS, or use `bun preview --target web` for browser preview.');
        }
        if (!(await commandAvailable('swiftc', ['--version']))) {
            addMissing(result, 'swiftc', 'Install Xcode Command Line Tools with `xcode-select --install`.');
        }
        return result;
    }

    if (normalized === 'ios') {
        if (platform !== 'darwin') {
            addMissing(result, 'macOS host', 'Run iOS preview from macOS with Xcode installed.');
        }
        if (!(await commandAvailable('xcodebuild', ['-version']))) {
            addMissing(result, 'xcodebuild', 'Install Xcode from the App Store and run `xcode-select --switch /Applications/Xcode.app` if needed.');
        }
        return result;
    }

    if (normalized === 'linux') {
        if (platform !== 'linux') {
            addMissing(result, 'Linux host', 'Run Linux native preview on Linux, or use `bun preview --target web` for browser preview.');
        }
        if (!(await commandAvailable('pkg-config', ['--exists', 'gtk+-3.0']))) {
            addMissing(result, 'GTK 3 development headers', 'Install GTK 3 development headers, for example `sudo apt-get install libgtk-3-dev`.');
        }
        if (!(await commandAvailable('pkg-config', ['--exists', 'webkit2gtk-4.1']))) {
            addMissing(result, 'WebKitGTK 4.1 development headers', 'Install WebKitGTK 4.1 development headers, for example `sudo apt-get install libwebkit2gtk-4.1-dev`.');
        }
        return result;
    }

    if (normalized === 'windows') {
        if (platform !== 'win32') {
            addMissing(result, 'Windows host', 'Run Windows native preview on Windows with WebView2 installed.');
        }
        const programFilesX86 = env['ProgramFiles(x86)'] ?? '';
        const webview2Runtime = programFilesX86
            ? path.join(programFilesX86, 'Microsoft', 'EdgeWebView', 'Application')
            : '';
        if (!webview2Runtime || !(await pathExists(webview2Runtime))) {
            addMissing(result, 'Microsoft Edge WebView2 Runtime', 'Install the Microsoft Edge WebView2 Runtime and Visual Studio C++ workload.');
        }
        return result;
    }

    if (normalized === 'android') {
        const sdkRoot = env.ANDROID_HOME || env.ANDROID_SDK_ROOT || '';
        if (!sdkRoot || !(await pathExists(sdkRoot))) {
            addMissing(result, 'Android SDK', 'Install Android Studio and set ANDROID_HOME or ANDROID_SDK_ROOT.');
        }
        if (!(await commandAvailable('java', ['-version']))) {
            addMissing(result, 'JDK 17+', 'Install JDK 17 or newer before previewing Android native targets.');
        }
        return result;
    }

    return result;
}

/**
 * @param {string} target
 * @param {NativePreviewCheckResult} result
 */
export function formatNativePreviewCheckFailure(target, result) {
    return [
        `Cannot preview native target '${target}' on this machine yet.`,
        result.missing.length ? `Missing: ${result.missing.join(', ')}` : '',
        ...result.suggestions.map((suggestion) => `- ${suggestion}`),
    ].filter(Boolean).join('\n');
}
