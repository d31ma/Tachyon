// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { access, mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'fs/promises';
import { Buffer } from 'node:buffer';
import path from 'path';
import { tmpdir } from 'os';
import { inflateSync } from 'node:zlib';

const timedTest = /** @type {any} */ (test);
/** @type {string[]} */
const tempDirs = [];
const bundleEntrypoint = path.join(process.cwd(), 'src', 'cli', 'bundle.js');
const nativeEntrypoint = path.join(process.cwd(), 'src', 'cli', 'native-bundle.js');

/** @param {ReadableStream<Uint8Array> | null | undefined} stream */
async function decode(stream) {
    return stream ? await new Response(stream).text() : '';
}

/**
 * @param {string} filePath
 * @param {number} x
 * @param {number} y
 */
async function readPngAlpha(filePath, x, y) {
    const bytes = new Uint8Array(await readFile(filePath));
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    let offset = 8;
    let width = 0;
    /** @type {Uint8Array[]} */
    const idatChunks = [];
    while (offset < bytes.length) {
        const length = view.getUint32(offset);
        const type = String.fromCharCode(...bytes.subarray(offset + 4, offset + 8));
        const data = bytes.subarray(offset + 8, offset + 8 + length);
        if (type === 'IHDR')
            width = new DataView(data.buffer, data.byteOffset, data.byteLength).getUint32(0);
        if (type === 'IDAT')
            idatChunks.push(data);
        offset += 12 + length;
    }
    const raw = inflateSync(Buffer.concat(idatChunks.map((chunk) => Buffer.from(chunk))));
    const rowLength = width * 4 + 1;
    expect(raw[y * rowLength]).toBe(0);
    return raw[y * rowLength + 1 + x * 4 + 3];
}

/**
 * @param {string} cwd
 * @param {string[]} args
 */
async function runCli(cwd, args) {
    const proc = Bun.spawn(args, {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe',
        env: process.env,
    });
    const [stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited,
    ]);
    if (exitCode !== 0)
        throw new Error(`${args.join(' ')} failed\n${stdout}\n${stderr}`);
    return { stdout, stderr };
}

async function createFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-native-bundle-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'native-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), '<main><h1>Native Fixture</h1></main>');
    return root;
}

afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

timedTest('tac.bundle ships a macOS native host at dist/<target>', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'macos']);

    // The native host occupies dist/macos/ directly — no separate web-bundle or
    // `-native` dir. Its web assets live under Resources/.
    await expect(access(path.join(cwd, 'dist', 'macos-native'))).rejects.toBeDefined();

    const nativeHtml = await readFile(path.join(cwd, 'dist', 'macos', 'Resources', 'index.html'), 'utf8');
    expect(nativeHtml).toContain('Native Fixture');
    expect(nativeHtml).toContain('src="./spa-renderer.js"');
    expect(nativeHtml).toContain('rel="icon" type="image/svg+xml" href="./shared/assets/favicon.svg"');
    expect(nativeHtml).toContain('<meta name="tachyon-target" content="macos">');
    expect(nativeHtml).toContain('<meta name="tachyon-platform" content="desktop">');
    expect(nativeHtml).toContain('<meta name="tachyon-environment" content="macos">');
    expect(nativeHtml).toContain('<meta name="tachyon-os" content="macos">');
    expect(nativeHtml).toContain('<meta name="tachyon-native-capabilities" content="app.info,auth.verifyUser,clipboard.readText,clipboard.writeText,fs.paths,openUrl,secrets.delete,secrets.get,secrets.set">');
    const spaRenderer = await readFile(path.join(cwd, 'dist', 'macos', 'Resources', 'spa-renderer.js'), 'utf8');
    expect(spaRenderer).toContain('tachyon-target');
    expect(spaRenderer).toContain('tachyon-platform');
    expect(spaRenderer).toContain('tachyon-os');
    expect(spaRenderer).toContain('browserOS');
    expect(spaRenderer).toContain('location.protocol==="file:"');
    expect(spaRenderer).toContain('.endsWith("/index.html")');
    expect(spaRenderer).toContain('location.protocol!=="file:"');

    const hostManifest = JSON.parse(await readFile(path.join(cwd, 'dist', 'macos', 'tachyon.host.json'), 'utf8'));
    expect(hostManifest.target).toBe('macos');
    expect(hostManifest.entry).toBe('Resources/index.html');
    expect(hostManifest.platformApiVersion).toBe(1);
    expect(hostManifest.bridgeVersion).toBe(2);
    expect(hostManifest.hostCapabilities).toEqual(expect.arrayContaining([
        'app.info', 'auth.verifyUser', 'clipboard.readText', 'clipboard.writeText', 'fs.paths', 'openUrl', 'secrets.get', 'secrets.set', 'secrets.delete',
    ]));
    expect(hostManifest.hostCapabilities).not.toContain('shell.exec');
    expect(hostManifest.rawHostCapabilities).toEqual([]);
    expect(hostManifest.requestedDevicePermissions).toEqual([]);
    expect(hostManifest.capabilities).toContainEqual({
        route: '*',
        capability: 'app.info',
        descriptor: {},
    });
    expect(hostManifest.capabilities).toContainEqual({
        route: '*',
        capability: 'clipboard.writeText',
        descriptor: {},
    });
    expect(hostManifest.capabilities).not.toContainEqual(expect.objectContaining({ capability: 'shell.exec' }));

    const swiftSource = await readFile(path.join(cwd, 'dist', 'macos', 'Sources', 'TachyonApp.swift'), 'utf8');
    expect(swiftSource).toContain('WKWebView');
    expect(swiftSource).toContain('import Security');
    expect(swiftSource).toContain('case "secrets.get"');
    expect(swiftSource).toContain('auth.verifyUser');
    expect(swiftSource).toContain('NSPasteboard.general');
    expect(swiftSource).toContain('case "openUrl"');
    expect(swiftSource).toContain('Native capability is not enabled: ');
    expect(swiftSource).toContain('openUrl requires an http(s) URL');
    expect(swiftSource).toContain('forMainFrameOnly: true');
    expect(swiftSource).toContain('MessageHandler.shared.webView = webView');
    expect(swiftSource).toContain('config.setURLSchemeHandler(TachyonSchemeHandler(), forURLScheme: "tachyon")');
    expect(swiftSource).toContain('tachyon://localhost/');
    expect(swiftSource).not.toContain('loadFileURL');
    expect(swiftSource).toContain('enum TachyonMain');
    expect(swiftSource).toContain('app.delegate = appDelegate');
    expect(swiftSource).toContain('app.run()');
    expect(swiftSource).toContain('for iconExtension in ["icns", "png"]');
    expect(swiftSource).toContain('NSApp.applicationIconImage = icon');
    const infoPlist = await readFile(path.join(cwd, 'dist', 'macos', 'TachyonApp', 'Info.plist'), 'utf8');
    expect(infoPlist).toContain('<key>CFBundleDisplayName</key>');
    expect(infoPlist).toContain('<key>CFBundleIconFile</key>');
    expect(infoPlist).toContain('<string>TachyonIcon</string>');
    expect(infoPlist).not.toContain('<key>CFBundleIconName</key>');
    expect(await readFile(path.join(cwd, 'dist', 'macos', 'TachyonApp', 'PkgInfo'), 'utf8')).toBe('APPL????');

    const buildScriptPath = path.join(cwd, 'dist', 'macos', 'build.sh');
    const buildScriptMode = (await stat(buildScriptPath)).mode;
    expect(buildScriptMode & 0o111).not.toBe(0);
    const buildScript = await readFile(buildScriptPath, 'utf8');
    expect(buildScript).toContain('codesign --force --deep --sign -');
    expect(buildScript).toContain('Contents/PkgInfo');
    expect(buildScript).toContain('xattr -cr');
    await expect(access(path.join(cwd, 'dist', 'macos', 'Resources', 'TachyonIcon.svg'))).resolves.toBeNull();
    await expect(access(path.join(cwd, 'dist', 'macos', 'Resources', 'TachyonIcon.ico'))).resolves.toBeNull();
    await expect(access(path.join(cwd, 'dist', 'macos', 'Resources', 'TachyonIcon.icns'))).resolves.toBeNull();
    await expect(access(path.join(cwd, 'dist', 'macos', 'Icons', 'macos', 'TachyonIcon.iconset', 'icon_512x512@2x.png'))).resolves.toBeNull();
    expect(await readPngAlpha(path.join(cwd, 'dist', 'macos', 'Resources', 'TachyonIcon.png'), 0, 0)).toBe(0);
    expect(await readPngAlpha(path.join(cwd, 'dist', 'macos', 'Resources', 'TachyonIcon.png'), 0, 256)).toBe(0);
    expect(await readPngAlpha(path.join(cwd, 'dist', 'macos', 'Resources', 'TachyonIcon.png'), 80, 256)).toBe(255);
    expect(await readPngAlpha(path.join(cwd, 'dist', 'macos', 'Resources', 'TachyonIcon.png'), 256, 256)).toBe(255);
    expect(await readPngAlpha(path.join(cwd, 'dist', 'macos', 'Assets.xcassets', 'AppIcon.appiconset', 'tachyon-ios-marketing@1x.png'), 0, 0)).toBe(255);
    if (process.platform === 'darwin') {
        await runCli(cwd, [
            'iconutil',
            '-c',
            'iconset',
            path.join(cwd, 'dist', 'macos', 'Resources', 'TachyonIcon.icns'),
            '-o',
            path.join(cwd, 'dist', 'macos', 'icon-inspect.iconset'),
        ]);
        await expect(access(path.join(cwd, 'dist', 'macos', 'icon-inspect.iconset', 'icon_512x512@2x.png'))).resolves.toBeNull();
    }

    await writeFile(path.join(cwd, 'dist', 'macos', 'stale.txt'), 'stale native output');
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'macos']);
    await expect(access(path.join(cwd, 'dist', 'macos', 'stale.txt'))).rejects.toBeDefined();
});

timedTest('tac.bundle can skip native host generation for native targets', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'ios', '--skip-native-host']);

    // With the host skipped, dist/ios/ keeps the plain web bundle (no host).
    await expect(access(path.join(cwd, 'dist', 'ios', 'index.html'))).resolves.toBeNull();
    await expect(access(path.join(cwd, 'dist', 'ios', 'tachyon.host.json'))).rejects.toBeDefined();
    await expect(access(path.join(cwd, 'dist', 'ios-native'))).rejects.toBeDefined();
});

timedTest('building one target leaves other targets\' dist output intact', { timeout: 60000 }, async () => {
    const cwd = await createFixture();
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'web']);
    const webIndex = path.join(cwd, 'dist', 'web', 'index.html');
    await expect(access(webIndex)).resolves.toBeNull();
    const webIndexBefore = await readFile(webIndex, 'utf8');

    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'android', '--skip-native-host']);

    // dist/web from the earlier run survives the android build untouched.
    await expect(access(webIndex)).resolves.toBeNull();
    expect(await readFile(webIndex, 'utf8')).toBe(webIndexBefore);
    await expect(access(path.join(cwd, 'dist', 'android', 'index.html'))).resolves.toBeNull();
});

timedTest('tac.native-bundle generates Android host from existing target assets', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'android', '--skip-native-host']);
    await runCli(cwd, ['bun', nativeEntrypoint, '--target', 'android']);

    const activity = await readFile(path.join(cwd, 'dist', 'android', 'app', 'src', 'main', 'java', 'ma', 'del', 'tachyon', 'nativefixture', 'MainActivity.kt'), 'utf8');
    expect(activity).toContain('class MainActivity : Activity()');
    expect(activity).toContain('TachyonWebViewClient');
    expect(activity).toContain('https://appassets.androidapp.com/');
    expect(activity).toContain('WebViewAssetLoader');
    expect(activity).toContain('"fs.readText"');
    expect(activity).toContain('"clipboard.writeText"');
    expect(activity).toContain('"share.text"');
    expect(activity).toContain('HapticFeedbackConstants.CONFIRM');
    expect(activity).toContain('class DeviceWebChromeClient');
    expect(activity).toContain('shell.exec is not available on Android native hosts');
    expect(activity).not.toContain('AppCompatActivity');
    // Secure storage (Android Keystore) and user verification (framework
    // BiometricPrompt) ship on Android at parity with iOS/macOS.
    expect(activity).toContain('"secrets.get"');
    expect(activity).toContain('AndroidKeyStore');
    expect(activity).toContain('AES/GCM/NoPadding');
    expect(activity).toContain('android.hardware.biometrics.BiometricPrompt');
    expect(activity).toContain('private fun verifyUser(');
    expect(activity).toContain('context is android.content.ContextWrapper');
    expect(activity).toContain('context = context.baseContext');

    const gradle = await readFile(path.join(cwd, 'dist', 'android', 'app', 'build.gradle.kts'), 'utf8');
    expect(gradle).toContain('android.sourceSets["main"].assets.srcDir("$rootDir/Resources")');
    expect(gradle).not.toContain('androidx.appcompat');
    const androidManifest = await readFile(path.join(cwd, 'dist', 'android', 'app', 'src', 'main', 'AndroidManifest.xml'), 'utf8');
    expect(androidManifest).toContain('android:icon="@mipmap/ic_launcher"');
    expect(androidManifest).toContain('android.permission.USE_BIOMETRIC');
    const adaptiveIcon = await readFile(path.join(cwd, 'dist', 'android', 'app', 'src', 'main', 'res', 'mipmap-anydpi-v26', 'ic_launcher.xml'), 'utf8');
    expect(adaptiveIcon).toContain('<adaptive-icon');
    expect(adaptiveIcon).toContain('@drawable/ic_launcher_background');
    await expect(access(path.join(cwd, 'dist', 'android', 'app', 'src', 'main', 'res', 'drawable', 'ic_launcher_foreground.xml'))).resolves.toBeNull();

    const hostManifest = JSON.parse(await readFile(path.join(cwd, 'dist', 'android', 'tachyon.host.json'), 'utf8'));
    expect(hostManifest.target).toBe('android');
    expect(hostManifest.hostCapabilities).toContain('share.text');
    expect(hostManifest.hostCapabilities).toContain('secrets.get');
    expect(hostManifest.hostCapabilities).toContain('auth.verifyUser');
    expect(hostManifest.hostCapabilities).not.toContain('fs.writeText');
    expect(hostManifest.hostCapabilities).not.toContain('shell.exec');
    expect(hostManifest.rawHostCapabilities).toEqual([]);
    expect(hostManifest.requestedDevicePermissions).toEqual([]);
    expect(activity).toContain('setOf("https://appassets.androidapp.com")');
    expect(activity).toContain('if (!isMainFrame');
    expect(activity).toContain('request.grant(allowedResources)');
    expect(activity).not.toContain('request.grant(request.resources)');
});

timedTest('native hosts generate platform raw capability dispatchers', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await writeFile(path.join(cwd, 'package.json'), JSON.stringify({
        name: 'native-fixture',
        private: true,
        tachyon: { nativeCapabilities: ['fs.readText', 'fs.writeText', 'fs.readDir', 'fs.stat', 'fs.mkdir', 'fs.remove', 'shell.exec'] },
    }, null, 2));
    await runCli(cwd, ['bun', bundleEntrypoint, '--targets', 'windows,linux,ios']);

    const windowsSource = await readFile(path.join(cwd, 'dist', 'windows', 'src', 'main.cpp'), 'utf8');
    expect(windowsSource).toContain('HandleNativeCapability');
    expect(windowsSource).toContain('fs.readText');
    expect(windowsSource).toContain('fs.stat');
    expect(windowsSource).toContain('fs.mkdir');
    expect(windowsSource).toContain('fs.remove');
    expect(windowsSource).toContain('if (ec) throw std::runtime_error("Unable to read file size")');
    expect(windowsSource).toContain('shell.exec');
    expect(windowsSource).toContain('ExtractJsonStringArray');
    expect(windowsSource).toContain('CreateProcessA');
    expect(windowsSource).toContain('ReadClipboardText');
    expect(windowsSource).toContain('ShellExecuteW');
    expect(windowsSource).toContain('Native capability is not enabled: ');
    expect(windowsSource).toContain('openUrl requires an http(s) URL');
    const windowsCmake = await readFile(path.join(cwd, 'dist', 'windows', 'CMakeLists.txt'), 'utf8');
    expect(windowsCmake).toContain('src/app.rc');
    await expect(access(path.join(cwd, 'dist', 'windows', 'src', 'app.rc'))).resolves.toBeNull();
    await expect(access(path.join(cwd, 'dist', 'windows', 'Resources', 'TachyonIcon.ico'))).resolves.toBeNull();

    const linuxSource = await readFile(path.join(cwd, 'dist', 'linux', 'src', 'main.c'), 'utf8');
    expect(linuxSource).toContain('handle_native_message');
    expect(linuxSource).toContain('fs.readText');
    expect(linuxSource).toContain('fs.stat');
    expect(linuxSource).toContain('remove_path_recursive');
    expect(linuxSource).toContain('(size_t)length >= sizeof(child)');
    expect(linuxSource).toContain('make_directory_recursive');
    expect(linuxSource).toContain('shell.exec');
    expect(linuxSource).toContain('extract_json_string_array');
    expect(linuxSource).toContain('execvp(command');
    expect(linuxSource).toContain('gtk_clipboard_wait_for_text');
    expect(linuxSource).toContain('gtk_show_uri_on_window');
    expect(linuxSource).toContain('WEBKIT_USER_CONTENT_INJECT_TOP_FRAME');
    expect(linuxSource).toContain('Native capability is not enabled');
    expect(linuxSource).toContain('gtk_window_set_icon_from_file');
    await expect(access(path.join(cwd, 'dist', 'linux', 'Resources', 'TachyonIcon.png'))).resolves.toBeNull();

    const iosSource = await readFile(path.join(cwd, 'dist', 'ios', 'Sources', 'TachyonWebView.swift'), 'utf8');
    expect(iosSource).toContain('fs.readText');
    expect(iosSource).toContain('fs.stat');
    expect(iosSource).toContain('fs.mkdir');
    expect(iosSource).toContain('fs.remove');
    expect(iosSource).toContain('clipboard.writeText');
    expect(iosSource).toContain('UIActivityViewController');
    expect(iosSource).toContain('UIImpactFeedbackGenerator');
    expect(iosSource).toContain('shell.exec is not available on iOS native hosts');
    expect(iosSource).toContain('Native bridge requests must come from the main frame');
    await expect(access(path.join(cwd, 'dist', 'ios', 'Assets.xcassets', 'AppIcon.appiconset', 'Contents.json'))).resolves.toBeNull();
});

timedTest('native raw capabilities are explicit and invalid declarations fail closed', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await writeFile(path.join(cwd, 'package.json'), JSON.stringify({
        name: 'native-fixture',
        private: true,
        tachyon: { nativeCapabilities: ['shell.exec', 'fs.readText'] },
    }, null, 2));
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'macos']);
    const manifest = JSON.parse(await readFile(path.join(cwd, 'dist', 'macos', 'tachyon.host.json'), 'utf8'));
    expect(manifest.rawHostCapabilities).toEqual(['fs.readText', 'shell.exec']);
    const source = await readFile(path.join(cwd, 'dist', 'macos', 'Sources', 'TachyonApp.swift'), 'utf8');
    expect(source).toContain('private let allowedCapabilities = Set(');
    expect(source).toContain('"fs.readText"');
    expect(source).toContain('"shell.exec"');

    await writeFile(path.join(cwd, 'package.json'), JSON.stringify({
        name: 'native-fixture',
        tachyon: { appName: 'Configured App' },
        tac: { nativeCapabilities: ['fs.readDir'] },
    }));
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'macos']);
    const legacyManifest = JSON.parse(await readFile(path.join(cwd, 'dist', 'macos', 'tachyon.host.json'), 'utf8'));
    expect(legacyManifest.rawHostCapabilities).toEqual(['fs.readDir']);

    await writeFile(path.join(cwd, 'package.json'), JSON.stringify({
        name: 'native-fixture',
        tachyon: { nativeCapabilities: ['process.spawn'] },
    }));
    await expect(runCli(cwd, ['bun', bundleEntrypoint, '--target', 'macos']))
        .rejects.toThrow("tachyon.nativeCapabilities contains unsupported value 'process.spawn'");
});

timedTest('native device permissions are opt-in and produce platform declarations', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await writeFile(path.join(cwd, 'package.json'), JSON.stringify({
        name: 'native-fixture',
        private: true,
        tachyon: { devicePermissions: ['camera', 'microphone', 'location', 'notifications'] },
    }, null, 2));

    await runCli(cwd, ['bun', bundleEntrypoint, '--targets', 'android,ios']);

    const androidManifest = await readFile(path.join(cwd, 'dist', 'android', 'app', 'src', 'main', 'AndroidManifest.xml'), 'utf8');
    expect(androidManifest).toContain('android.permission.CAMERA');
    expect(androidManifest).toContain('android.permission.RECORD_AUDIO');
    expect(androidManifest).toContain('android.permission.ACCESS_FINE_LOCATION');
    expect(androidManifest).toContain('android.permission.POST_NOTIFICATIONS');
    const androidHost = JSON.parse(await readFile(path.join(cwd, 'dist', 'android', 'tachyon.host.json'), 'utf8'));
    expect(androidHost.requestedDevicePermissions).toEqual(['camera', 'location', 'microphone', 'notifications']);

    const iosProject = await readFile(path.join(cwd, 'dist', 'ios', 'project.yml'), 'utf8');
    expect(iosProject).toContain('NSCameraUsageDescription');
    expect(iosProject).toContain('NSMicrophoneUsageDescription');
    expect(iosProject).toContain('NSLocationWhenInUseUsageDescription');
});
