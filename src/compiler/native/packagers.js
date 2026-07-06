// @ts-check
/**
 * Native artifact packagers.
 *
 * After a native host project is generated at `dist/<target>/`, these
 * packagers turn it into a distributable artifact — an `.apk` for Android and
 * an `.ipa` for iOS — using the system toolchains. Packaging is best-effort
 * by design: when a toolchain is missing the packager reports what to
 * install and the generated project remains fully usable by hand.
 */

import path from 'path';
import { access, copyFile, mkdir, readdir, rename, rm, writeFile } from 'fs/promises';
import { homedir } from 'os';

/**
 * @typedef {{ artifactPaths: string[] }} PackageSuccess
 * @typedef {{ skipped: string }} PackageSkip
 * @typedef {PackageSuccess | PackageSkip} PackageResult
 * @typedef {{ target: string, projectRoot: string, appName: string, version: string }} PackageOptions
 */

/** @param {string} candidate */
async function pathExists(candidate) {
    try {
        await access(candidate);
        return true;
    }
    catch {
        return false;
    }
}

/**
 * @param {string[]} cmd
 * @param {{ cwd?: string, env?: Record<string, string | undefined> }} [options]
 * @returns {Promise<{ exitCode: number, stdout: string, stderr: string }>}
 */
async function run(cmd, options = {}) {
    const proc = Bun.spawn({
        cmd,
        cwd: options.cwd,
        env: { ...process.env, ...options.env },
        stdout: 'pipe',
        stderr: 'pipe',
    });
    const [stdout, stderr, exitCode] = await Promise.all([
        new Response(proc.stdout).text(),
        new Response(proc.stderr).text(),
        proc.exited,
    ]);
    return { exitCode, stdout, stderr };
}

/** @param {string} command @param {string[]} [args] */
async function commandAvailable(command, args = ['--version']) {
    try {
        return (await run([command, ...args])).exitCode === 0;
    }
    catch {
        return false;
    }
}

/** @param {{ stdout: string, stderr: string }} result @param {number} [lines] */
function outputTail(result, lines = 25) {
    return `${result.stdout}\n${result.stderr}`.trim().split('\n').slice(-lines).join('\n');
}

// ── Android ────────────────────────────────────────────────────────────

/** @returns {Promise<string>} Android SDK root, or '' when none is found. */
async function androidSdkRoot() {
    const candidates = [
        process.env.ANDROID_HOME,
        process.env.ANDROID_SDK_ROOT,
        path.join(homedir(), 'Library', 'Android', 'sdk'),
        path.join(homedir(), 'Android', 'Sdk'),
        '/opt/homebrew/share/android-commandlinetools',
        '/usr/local/share/android-commandlinetools',
    ];
    for (const candidate of candidates) {
        if (candidate && await pathExists(candidate))
            return candidate;
    }
    return '';
}

/** @returns {Promise<string>} Gradle executable, or '' when none is found. */
async function gradleExecutable() {
    const candidates = [
        process.env.TAC_ANDROID_GRADLE,
        'gradle',
        '/opt/homebrew/opt/gradle@8/bin/gradle',
        '/opt/homebrew/bin/gradle',
        '/usr/local/bin/gradle',
    ];
    for (const candidate of candidates) {
        if (candidate && await commandAvailable(candidate, ['--version']))
            return candidate;
    }
    return '';
}

/**
 * Build a signed `.apk` from the generated Gradle project. Release builds
 * fall back to the debug keystore when `TAC_ANDROID_KEYSTORE` is not set, so
 * the artifact is installable out of the box.
 * @param {PackageOptions} options
 * @returns {Promise<PackageResult>}
 */
async function packageAndroid(options) {
    const sdkRoot = await androidSdkRoot();
    if (!sdkRoot)
        return { skipped: 'Android SDK not found — install Android Studio or `brew install --cask android-commandlinetools` and set ANDROID_HOME.' };
    const gradle = await gradleExecutable();
    if (!gradle)
        return { skipped: 'Gradle not found — `brew install gradle@8` (or set TAC_ANDROID_GRADLE).' };

    // Pin the SDK for AGP regardless of the caller's environment.
    await writeFile(path.join(options.projectRoot, 'local.properties'), `sdk.dir=${sdkRoot}\n`);

    const build = await run([gradle, '--no-daemon', '-p', options.projectRoot, 'assembleRelease'], {
        env: { ANDROID_HOME: sdkRoot, ANDROID_SDK_ROOT: sdkRoot },
    });
    if (build.exitCode !== 0) {
        throw new Error(`gradle assembleRelease failed (exit ${build.exitCode}):\n${outputTail(build)}`);
    }

    const apkDir = path.join(options.projectRoot, 'app', 'build', 'outputs', 'apk', 'release');
    const apks = (await readdir(apkDir)).filter((entry) => entry.endsWith('.apk'));
    if (apks.length === 0) {
        throw new Error(`gradle assembleRelease produced no .apk under ${apkDir}`);
    }
    const artifactPath = path.join(options.projectRoot, `${options.appName}-${options.version}.apk`);
    await copyFile(path.join(apkDir, apks[0]), artifactPath);
    return { artifactPaths: [artifactPath] };
}

// ── iOS ────────────────────────────────────────────────────────────────

/** @returns {Promise<boolean>} True when full Xcode (not just CLT) is active. */
async function xcodeAvailable() {
    const selected = await run(['xcode-select', '-p']);
    if (selected.exitCode !== 0 || selected.stdout.includes('CommandLineTools'))
        return false;
    return await commandAvailable('xcodebuild', ['-version']);
}

/**
 * Build an `.ipa` from the generated Sources + project.yml via xcodegen and
 * xcodebuild. Without `TAC_IOS_TEAM_ID` the build is unsigned (sign later
 * with your distribution workflow); with it, xcodebuild signs automatically.
 * @param {PackageOptions} options
 * @returns {Promise<PackageResult>}
 */
async function packageIOS(options) {
    if (!(await xcodeAvailable()))
        return { skipped: 'Full Xcode is required for .ipa export — install Xcode from the App Store and run `sudo xcode-select --switch /Applications/Xcode.app`.' };
    if (!(await commandAvailable('xcodegen', ['--version'])))
        return { skipped: 'xcodegen not found — `brew install xcodegen`.' };
    if (!(await pathExists(path.join(options.projectRoot, 'project.yml'))))
        return { skipped: 'project.yml missing from the generated iOS host — re-run the bundle.' };

    const generate = await run(['xcodegen', 'generate'], { cwd: options.projectRoot });
    if (generate.exitCode !== 0) {
        throw new Error(`xcodegen generate failed (exit ${generate.exitCode}):\n${outputTail(generate)}`);
    }

    const scheme = options.appName;
    const teamId = process.env.TAC_IOS_TEAM_ID || '';
    const derivedData = path.join(options.projectRoot, 'build');
    const buildCmd = [
        'xcodebuild',
        '-project', path.join(options.projectRoot, `${scheme}.xcodeproj`),
        '-scheme', scheme,
        '-configuration', 'Release',
        '-destination', 'generic/platform=iOS',
        '-derivedDataPath', derivedData,
        ...(teamId
            ? [`DEVELOPMENT_TEAM=${teamId}`, '-allowProvisioningUpdates']
            : ['CODE_SIGN_IDENTITY=', 'CODE_SIGNING_REQUIRED=NO', 'CODE_SIGNING_ALLOWED=NO']),
        'build',
    ];
    const build = await run(buildCmd, { cwd: options.projectRoot });
    if (build.exitCode !== 0) {
        throw new Error(`xcodebuild failed (exit ${build.exitCode}):\n${outputTail(build)}`);
    }

    const appPath = path.join(derivedData, 'Build', 'Products', 'Release-iphoneos', `${scheme}.app`);
    if (!(await pathExists(appPath))) {
        throw new Error(`xcodebuild produced no app bundle at ${appPath}`);
    }

    // An .ipa is a zip with the app under Payload/.
    const payloadRoot = path.join(derivedData, 'ipa-staging');
    const payloadDir = path.join(payloadRoot, 'Payload');
    await rm(payloadRoot, { recursive: true, force: true });
    await mkdir(payloadDir, { recursive: true });
    const copy = await run(['ditto', appPath, path.join(payloadDir, `${scheme}.app`)]);
    if (copy.exitCode !== 0) {
        throw new Error(`ditto failed while staging the .ipa payload:\n${outputTail(copy)}`);
    }
    const artifactPath = path.join(options.projectRoot, `${options.appName}-${options.version}${teamId ? '' : '-unsigned'}.ipa`);
    await rm(artifactPath, { force: true });
    // --norsrc keeps AppleDouble sidecars (._Info.plist and friends) out of
    // the archive; store validation flags them.
    const zip = await run(['ditto', '-c', '-k', '--norsrc', '--keepParent', 'Payload', artifactPath], { cwd: payloadRoot });
    if (zip.exitCode !== 0) {
        throw new Error(`ditto failed while zipping the .ipa:\n${outputTail(zip)}`);
    }
    await rm(payloadRoot, { recursive: true, force: true });
    return { artifactPaths: [artifactPath] };
}

// ── macOS ──────────────────────────────────────────────────────────────

/**
 * Build the `.app` bundle with the host project's own build.sh (swiftc +
 * ad-hoc codesign).
 * @param {PackageOptions} options
 * @returns {Promise<PackageResult>}
 */
async function packageMacOS(options) {
    if (process.platform !== 'darwin')
        return { skipped: 'macOS apps can only be built on macOS.' };
    if (!(await commandAvailable('swiftc', ['--version'])))
        return { skipped: 'swiftc not found — install the Xcode Command Line Tools (`xcode-select --install`).' };

    const build = await run(['sh', 'build.sh'], { cwd: options.projectRoot });
    if (build.exitCode !== 0) {
        throw new Error(`macOS build.sh failed (exit ${build.exitCode}):\n${outputTail(build)}`);
    }
    const appPath = path.join(options.projectRoot, 'build', `${options.appName}.app`);
    if (!(await pathExists(appPath))) {
        throw new Error(`macOS build produced no app bundle at ${appPath}`);
    }
    return { artifactPaths: [appPath] };
}

// ── Linux ──────────────────────────────────────────────────────────────

/**
 * Build the WebKitGTK host with its own build.sh (CMake). The artifact is
 * the `build/<App>/` directory: the executable plus its Resources/.
 * @param {PackageOptions} options
 * @returns {Promise<PackageResult>}
 */
async function packageLinux(options) {
    if (process.platform !== 'linux')
        return { skipped: 'Linux apps can only be built on Linux.' };
    if (!(await commandAvailable('cmake', ['--version'])))
        return { skipped: 'cmake not found — `apt-get install cmake` (plus libgtk-3-dev and libwebkit2gtk-4.1-dev).' };
    if ((await run(['pkg-config', '--exists', 'gtk+-3.0', 'webkit2gtk-4.1'])).exitCode !== 0)
        return { skipped: 'GTK/WebKitGTK development packages not found — `apt-get install libgtk-3-dev libwebkit2gtk-4.1-dev`.' };

    const build = await run(['sh', 'build.sh'], { cwd: options.projectRoot });
    if (build.exitCode !== 0) {
        throw new Error(`Linux build.sh failed (exit ${build.exitCode}):\n${outputTail(build)}`);
    }
    const appDir = path.join(options.projectRoot, 'build', options.appName);
    if (!(await pathExists(path.join(appDir, options.appName)))) {
        throw new Error(`Linux build produced no executable under ${appDir}`);
    }
    return { artifactPaths: [appDir] };
}

// ── Windows ────────────────────────────────────────────────────────────

/**
 * @param {string} dir
 * @param {string} fileName
 * @returns {Promise<string>} Absolute path of the first match, or ''.
 */
async function findFileRecursive(dir, fileName) {
    /** @type {import('fs').Dirent[]} */
    let entries;
    try {
        entries = await readdir(dir, { withFileTypes: true });
    }
    catch {
        return '';
    }
    for (const entry of entries) {
        const entryPath = path.join(dir, entry.name);
        if (entry.isFile() && entry.name.toLowerCase() === fileName.toLowerCase())
            return entryPath;
        if (entry.isDirectory()) {
            const nested = await findFileRecursive(entryPath, fileName);
            if (nested) return nested;
        }
    }
    return '';
}

/** @returns {Promise<boolean>} True when VS 2022+ with the C++ workload is installed. */
async function visualStudioAvailable() {
    const programFilesX86 = process.env['ProgramFiles(x86)'] || 'C:/Program Files (x86)';
    const vswhere = path.join(programFilesX86, 'Microsoft Visual Studio', 'Installer', 'vswhere.exe');
    if (!(await pathExists(vswhere)))
        return false;
    const probe = await run([
        vswhere,
        '-latest',
        '-products', '*',
        '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
        '-property', 'installationPath',
    ]);
    return probe.exitCode === 0 && probe.stdout.trim().length > 0;
}

/**
 * Build the WebView2 host with its own build.bat (CMake + MSVC). The
 * artifact is the executable's output directory: `<App>.exe` beside its
 * Resources/, renamed to `<App>/`.
 * @param {PackageOptions} options
 * @returns {Promise<PackageResult>}
 */
async function packageWindows(options) {
    if (process.platform !== 'win32')
        return { skipped: 'Windows apps can only be built on Windows.' };
    if (!(await commandAvailable('cmake', ['--version'])))
        return { skipped: 'cmake not found — `winget install Kitware.CMake`.' };
    if (!(await visualStudioAvailable()))
        return { skipped: 'Visual Studio 2022 with the C++ workload not found — install it along with the WebView2 SDK.' };

    const build = await run(['cmd', '/c', 'build.bat'], { cwd: options.projectRoot });
    if (build.exitCode !== 0) {
        throw new Error(`Windows build.bat failed (exit ${build.exitCode}):\n${outputTail(build)}`);
    }

    const exePath = await findFileRecursive(path.join(options.projectRoot, 'build'), `${options.appName}.exe`);
    if (!exePath) {
        throw new Error(`Windows build produced no ${options.appName}.exe under ${path.join(options.projectRoot, 'build')}`);
    }
    // Ship the whole output directory (exe + Resources) under the app's name.
    let artifactDir = path.dirname(exePath);
    if (path.basename(artifactDir) !== options.appName) {
        const named = path.join(path.dirname(artifactDir), options.appName);
        await rm(named, { recursive: true, force: true });
        await rename(artifactDir, named);
        artifactDir = named;
    }
    return { artifactPaths: [artifactDir] };
}

// ── Dispatch ───────────────────────────────────────────────────────────

/** @type {Record<string, (options: PackageOptions) => Promise<PackageResult>>} */
const PACKAGERS = {
    android: packageAndroid,
    ios: packageIOS,
    macos: packageMacOS,
    linux: packageLinux,
    windows: packageWindows,
};

/** @param {string} target */
export function hasNativePackager(target) {
    return Object.hasOwn(PACKAGERS, target);
}

/**
 * @param {PackageOptions} options
 * @returns {Promise<PackageResult>}
 */
export async function packageNativeArtifact(options) {
    const packager = PACKAGERS[options.target];
    if (!packager)
        return { skipped: `No artifact packager for target '${options.target}'.` };
    return packager(options);
}
