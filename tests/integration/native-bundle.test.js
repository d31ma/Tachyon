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
const originalNativeCaps = process.env.TAC_NATIVE_CAPABILITIES;

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
    await mkdir(path.join(root, 'client', 'workers', 'native'), { recursive: true });
    await mkdir(path.join(root, 'client', 'workers', 'unsupported'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'native-fixture',
        private: true,
    }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), '<main><h1>Native Fixture</h1></main>');
    await writeFile(path.join(root, 'client', 'workers', 'native', 'tac.rs'), `
impl Handler {
    pub fn PATCH(request: Request) -> Json { json(request.body()) }
}
`);
    await writeFile(path.join(root, 'client', 'workers', 'native', 'OPTIONS.schema.json'), JSON.stringify({
        PATCH: {
            payload: { body: '^[\\s\\S]*$' },
            response: { result: '^[\\s\\S]*$' },
        },
    }, null, 2));
    await writeFile(path.join(root, 'client', 'workers', 'unsupported', 'tac.go'), `
type Handler struct{}

func (Handler) GET(request Request) int32 { return request.Len() }
`);
    return root;
}

afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
    if (originalNativeCaps === undefined)
        delete process.env.TAC_NATIVE_CAPABILITIES;
    else
        process.env.TAC_NATIVE_CAPABILITIES = originalNativeCaps;
});

timedTest('tac.bundle ships a macOS native host at dist/<target>', { timeout: 30000 }, async () => {
    process.env.TAC_NATIVE_CAPABILITIES = 'app.info';
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
    expect(nativeHtml).toContain('<meta name="tachyon-platform" content="macos">');
    expect(nativeHtml).toContain('<meta name="tachyon-environment" content="desktop">');
    expect(nativeHtml).toContain('<meta name="tachyon-os" content="macos">');
    expect(nativeHtml).toContain('<meta name="tachyon-native-workers" content="native">');
    expect(nativeHtml).toContain('<meta name="tachyon-native-capabilities" content="app.info">');
    const spaRenderer = await readFile(path.join(cwd, 'dist', 'macos', 'Resources', 'spa-renderer.js'), 'utf8');
    expect(spaRenderer).toContain('tachyon-target');
    expect(spaRenderer).toContain('tachyon-platform');
    expect(spaRenderer).toContain('tachyon-environment');
    expect(spaRenderer).toContain('browserOS');
    expect(spaRenderer).toContain('location.protocol==="file:"');
    expect(spaRenderer).toContain('.endsWith("/index.html")');
    expect(spaRenderer).toContain('location.protocol!=="file:"');

    const hostManifest = JSON.parse(await readFile(path.join(cwd, 'dist', 'macos', 'tachyon.host.json'), 'utf8'));
    expect(hostManifest.target).toBe('macos');
    expect(hostManifest.entry).toBe('Resources/index.html');
    expect(hostManifest.capabilities).toContainEqual({
        route: '*',
        capability: 'app.info',
        descriptor: {},
    });

    const swiftSource = await readFile(path.join(cwd, 'dist', 'macos', 'Sources', 'TachyonApp.swift'), 'utf8');
    expect(swiftSource).toContain('WKWebView');
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
    await expect(access(path.join(cwd, 'dist', 'macos', 'Resources', 'workers', 'unsupported', 'go'))).rejects.toBeDefined();

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
    expect(activity).toContain('shell.exec is not available on Android native hosts');
    expect(activity).not.toContain('AppCompatActivity');

    const gradle = await readFile(path.join(cwd, 'dist', 'android', 'app', 'build.gradle.kts'), 'utf8');
    expect(gradle).toContain('android.sourceSets["main"].assets.srcDir("$rootDir/Resources")');
    expect(gradle).not.toContain('androidx.appcompat');
    const androidManifest = await readFile(path.join(cwd, 'dist', 'android', 'app', 'src', 'main', 'AndroidManifest.xml'), 'utf8');
    expect(androidManifest).toContain('android:icon="@mipmap/ic_launcher"');
    const adaptiveIcon = await readFile(path.join(cwd, 'dist', 'android', 'app', 'src', 'main', 'res', 'mipmap-anydpi-v26', 'ic_launcher.xml'), 'utf8');
    expect(adaptiveIcon).toContain('<adaptive-icon');
    expect(adaptiveIcon).toContain('@drawable/ic_launcher_background');
    await expect(access(path.join(cwd, 'dist', 'android', 'app', 'src', 'main', 'res', 'drawable', 'ic_launcher_foreground.xml'))).resolves.toBeNull();

    const hostManifest = JSON.parse(await readFile(path.join(cwd, 'dist', 'android', 'tachyon.host.json'), 'utf8'));
    expect(hostManifest.target).toBe('android');
});

timedTest('native hosts generate platform raw capability dispatchers', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await runCli(cwd, ['bun', bundleEntrypoint, '--targets', 'windows,linux,ios']);

    const windowsSource = await readFile(path.join(cwd, 'dist', 'windows', 'src', 'main.cpp'), 'utf8');
    expect(windowsSource).toContain('HandleNativeCapability');
    expect(windowsSource).toContain('fs.readText');
    expect(windowsSource).toContain('shell.exec');
    expect(windowsSource).toContain('ExtractJsonStringArray');
    expect(windowsSource).toContain('CreateProcessA');
    expect(windowsSource).toContain('tachyon.worker');
    const windowsCmake = await readFile(path.join(cwd, 'dist', 'windows', 'CMakeLists.txt'), 'utf8');
    expect(windowsCmake).toContain('src/app.rc');
    await expect(access(path.join(cwd, 'dist', 'windows', 'src', 'app.rc'))).resolves.toBeNull();
    await expect(access(path.join(cwd, 'dist', 'windows', 'Resources', 'TachyonIcon.ico'))).resolves.toBeNull();

    const linuxSource = await readFile(path.join(cwd, 'dist', 'linux', 'src', 'main.c'), 'utf8');
    expect(linuxSource).toContain('handle_native_message');
    expect(linuxSource).toContain('fs.readText');
    expect(linuxSource).toContain('shell.exec');
    expect(linuxSource).toContain('extract_json_string_array');
    expect(linuxSource).toContain('execvp(command');
    expect(linuxSource).toContain('tachyon.worker');
    expect(linuxSource).toContain('gtk_window_set_icon_from_file');
    await expect(access(path.join(cwd, 'dist', 'linux', 'Resources', 'TachyonIcon.png'))).resolves.toBeNull();

    const iosSource = await readFile(path.join(cwd, 'dist', 'ios', 'Sources', 'TachyonWebView.swift'), 'utf8');
    expect(iosSource).toContain('fs.readText');
    expect(iosSource).toContain('shell.exec is not available on iOS native hosts');
    await expect(access(path.join(cwd, 'dist', 'ios', 'Assets.xcassets', 'AppIcon.appiconset', 'Contents.json'))).resolves.toBeNull();
});

timedTest('desktop native hosts bundle Rust Tac worker executables', { timeout: 60000, skip: process.platform !== 'darwin' }, async () => {
    process.env.TAC_NATIVE_CAPABILITIES = 'app.info,tachyon.worker';
    const cwd = await createFixture();
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'macos']);

    const workerBinary = path.join(cwd, 'dist', 'macos', 'Resources', 'workers', 'native', 'rs');
    await expect(access(workerBinary)).resolves.toBeNull();
    const binaryStat = await stat(workerBinary);
    expect(binaryStat.isFile()).toBe(true);
    expect(binaryStat.mode & 0o111).not.toBe(0);

    const swiftSource = await readFile(path.join(cwd, 'dist', 'macos', 'Sources', 'TachyonApp.swift'), 'utf8');
    expect(swiftSource).toContain('case "tachyon.worker"');
    expect(swiftSource).toContain('runWorker(route:');
    expect(swiftSource).toContain('process.standardInput = stdin');
    expect(swiftSource).toContain('stdin.fileHandleForWriting.write(inputData)');
    expect(swiftSource).not.toContain('stdout.fileHandleForWriting.write(inputData)');
});
