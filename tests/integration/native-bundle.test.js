// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { access, mkdir, mkdtemp, readFile, rm, writeFile } from 'fs/promises';
import path from 'path';
import { tmpdir } from 'os';

const timedTest = /** @type {any} */ (test);
/** @type {string[]} */
const tempDirs = [];
const bundleEntrypoint = path.join(process.cwd(), 'src', 'cli', 'bundle.js');
const nativeEntrypoint = path.join(process.cwd(), 'src', 'cli', 'native-bundle.js');

/** @param {ReadableStream<Uint8Array> | null | undefined} stream */
async function decode(stream) {
    return stream ? await new Response(stream).text() : '';
}

/** @param {string} cwd @param {string[]} args */
async function runCli(cwd, args) {
    const proc = Bun.spawn(args, { cwd, stdout: 'pipe', stderr: 'pipe', env: process.env });
    const [stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout), decode(proc.stderr), proc.exited,
    ]);
    if (exitCode !== 0) throw new Error(`${args.join(' ')} failed\n${stdout}\n${stderr}`);
    return { stdout, stderr };
}

async function createFixture(page = '<main><h1>Native Fixture</h1></main>') {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-native-bundle-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'client', 'pages'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({ name: 'native-fixture', private: true }, null, 2));
    await writeFile(path.join(root, 'client', 'pages', 'tac.html'), page);
    return root;
}

afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

timedTest('tac.bundle ships a native-first macOS host at dist/<target>', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'macos']);

    await expect(access(path.join(cwd, 'dist', 'macos-native'))).rejects.toBeDefined();
    await expect(access(path.join(cwd, 'dist', 'macos', 'Resources', 'tachyon.native-ui.json'))).resolves.toBeNull();
    await expect(access(path.join(cwd, 'dist', 'macos', 'Resources', 'tachyon.native-controller.js'))).resolves.toBeNull();
    const manifest = JSON.parse(await readFile(path.join(cwd, 'dist', 'macos', 'tachyon.host.json'), 'utf8'));
    expect(manifest).toMatchObject({
        target: 'macos',
        entry: 'Resources/tachyon.native-ui.json',
        renderMode: 'native',
        hasWebViewFallbacks: false,
        hostCapabilities: expect.arrayContaining([
            'window.opacity',
            'window.clickThrough',
            'window.captureProtection',
            'shortcuts.register',
        ]),
        rawHostCapabilities: [],
    });
    expect(manifest.managedContentPolicy).not.toHaveProperty('presentation');
    expect(manifest.managedContentPolicy).not.toHaveProperty('layout');
    const source = await readFile(path.join(cwd, 'dist', 'macos', 'Sources', 'TachyonApp.swift'), 'utf8');
    expect(source).toContain('import SwiftUI');
    expect(source).toContain('import JavaScriptCore');
    expect(source).toContain('TachyonNativeNodeView');
    expect(source).not.toContain('import WebKit');

    await writeFile(path.join(cwd, 'dist', 'macos', 'stale.txt'), 'stale native output');
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'macos']);
    await expect(access(path.join(cwd, 'dist', 'macos', 'stale.txt'))).rejects.toBeDefined();
});

timedTest('automatic fallback keeps native siblings and adds only local WebView support', { timeout: 30000 }, async () => {
    const cwd = await createFixture('<main><h1>Native</h1><company-chart><canvas></canvas></company-chart><button>Refresh</button></main>');
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'macos']);
    const manifest = JSON.parse(await readFile(path.join(cwd, 'dist', 'macos', 'tachyon.host.json'), 'utf8'));
    expect(manifest).toMatchObject({ renderMode: 'native', hasWebViewFallbacks: true });
    const bundle = JSON.parse(await readFile(path.join(cwd, 'dist', 'macos', 'Resources', 'tachyon.native-ui.json'), 'utf8'));
    expect(bundle.webViewFallbacks).toEqual(['company-chart']);
    expect(bundle.routes[0].root.children).toMatchObject([
        { kind: 'element', tag: 'h1' },
        { kind: 'webview', tag: 'company-chart' },
        { kind: 'element', tag: 'button' },
    ]);
    const source = await readFile(path.join(cwd, 'dist', 'macos', 'Sources', 'TachyonApp.swift'), 'utf8');
    expect(source).toContain('TachyonHybridWebView');
    expect(source).toContain('TachyonNativeNodeView');
});

timedTest('tac.bundle can skip native host generation for native targets', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'ios', '--skip-native-host']);
    await expect(access(path.join(cwd, 'dist', 'ios', 'index.html'))).resolves.toBeNull();
    await expect(access(path.join(cwd, 'dist', 'ios', 'tachyon.native-ui.json'))).resolves.toBeNull();
    await expect(access(path.join(cwd, 'dist', 'ios', 'tachyon.host.json'))).rejects.toBeDefined();
});

timedTest('building one target leaves other targets\' dist output intact', { timeout: 60000 }, async () => {
    const cwd = await createFixture();
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'web']);
    const webIndex = path.join(cwd, 'dist', 'web', 'index.html');
    const webIndexBefore = await readFile(webIndex, 'utf8');
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'android', '--skip-native-host']);
    expect(await readFile(webIndex, 'utf8')).toBe(webIndexBefore);
    await expect(access(path.join(cwd, 'dist', 'android', 'tachyon.native-ui.json'))).resolves.toBeNull();
});

timedTest('tac.native-bundle generates a Compose host from existing native UI assets', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'android', '--skip-native-host']);
    await runCli(cwd, ['bun', nativeEntrypoint, '--target', 'android']);
    const activity = await readFile(path.join(cwd, 'dist', 'android', 'app', 'src', 'main', 'java', 'ma', 'del', 'tachyon', 'nativefixture', 'MainActivity.kt'), 'utf8');
    expect(activity).toContain('class MainActivity : ComponentActivity()');
    expect(activity).toContain('androidx.activity.compose.setContent');
    expect(activity).toContain('QuickJs.create()');
    expect(activity).not.toContain('android.webkit.WebView');
    const manifest = JSON.parse(await readFile(path.join(cwd, 'dist', 'android', 'tachyon.host.json'), 'utf8'));
    expect(manifest).toMatchObject({ target: 'android', renderMode: 'native', hasWebViewFallbacks: false });
});

timedTest('native-first hosts reject compatibility-only capabilities until native adapters exist', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await writeFile(path.join(cwd, 'package.json'), JSON.stringify({
        name: 'native-fixture', private: true,
        tachyon: { nativeCapabilities: ['fs.readText'], devicePermissions: ['camera'] },
    }, null, 2));
    await expect(runCli(cwd, ['bun', bundleEntrypoint, '--target', 'android']))
        .rejects.toThrow(/Native-first bundles do not expose nativeCapabilities, devicePermissions yet/i);
});

timedTest('native-first hosts reject standalone permission origins without an adapter', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await writeFile(path.join(cwd, 'package.json'), JSON.stringify({
        name: 'native-fixture', private: true,
        tachyon: { permissionOrigins: { camera: ['https://camera.example'] } },
    }, null, 2));
    await expect(runCli(cwd, ['bun', bundleEntrypoint, '--target', 'android']))
        .rejects.toThrow(/Native-first bundles do not expose permissionOrigins yet/i);
});

timedTest('macOS native-first host accepts and advertises concrete CLOAK capabilities', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await writeFile(path.join(cwd, 'package.json'), JSON.stringify({
        name: 'native-fixture', private: true,
        tachyon: {
            devicePermissions: ['microphone', 'screenCapture'],
            managedContentOrigins: ['https://chatgpt.com', 'https://claude.ai'],
            permissionOrigins: { microphone: ['https://chatgpt.com', 'https://claude.ai'] },
        },
    }, null, 2));
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'macos']);
    const manifest = JSON.parse(await readFile(path.join(cwd, 'dist', 'macos', 'tachyon.host.json'), 'utf8'));
    expect(manifest.hostCapabilities).toEqual(expect.arrayContaining([
        'contentSurface.open',
        'screenCapture.listWindows',
        'shortcuts.register',
        'window.opacity',
    ]));
    expect(manifest.requestedDevicePermissions).toEqual(['microphone', 'screenCapture']);
    expect(manifest.permissionOrigins).toEqual({ microphone: ['https://chatgpt.com', 'https://claude.ai'] });
    expect(manifest.managedContentPolicy.allowedOrigins).toEqual(['https://chatgpt.com', 'https://claude.ai']);
    expect(manifest.managedContentPolicy).toMatchObject({
        presentation: 'composed',
        layout: { mode: 'split', edge: 'right', ratio: 0.75 },
    });
    const source = await readFile(path.join(cwd, 'dist', 'macos', 'Sources', 'TachyonApp.swift'), 'utf8');
    expect(source).toContain('requestMediaCapturePermissionFor');
    expect(source).toContain('screenCapture.captureWindow');
    expect(source).toContain('globalShortcutMonitor');
});

timedTest('desktop native-first hosts reject declarations without target adapters', { timeout: 30000 }, async () => {
    const linuxCwd = await createFixture();
    await writeFile(path.join(linuxCwd, 'package.json'), JSON.stringify({
        name: 'native-fixture', private: true,
        tachyon: { devicePermissions: ['screenCapture'] },
    }, null, 2));
    await expect(runCli(linuxCwd, ['bun', bundleEntrypoint, '--target', 'linux']))
        .rejects.toThrow(/linux.+do not implement device permission: screenCapture/i);

    const windowsCwd = await createFixture();
    await writeFile(path.join(windowsCwd, 'package.json'), JSON.stringify({
        name: 'native-fixture', private: true,
        tachyon: { nativeCapabilities: ['fs.readText'] },
    }, null, 2));
    await expect(runCli(windowsCwd, ['bun', bundleEntrypoint, '--target', 'windows']))
        .rejects.toThrow(/windows.+do not implement raw nativeCapabilities/i);
});

timedTest('managed permission origins must be declared and least-privileged', { timeout: 30000 }, async () => {
    const missingPermission = await createFixture();
    await writeFile(path.join(missingPermission, 'package.json'), JSON.stringify({
        name: 'native-fixture', private: true,
        tachyon: {
            managedContentOrigins: ['https://chatgpt.com'],
            permissionOrigins: { microphone: ['https://chatgpt.com'] },
        },
    }, null, 2));
    await expect(runCli(missingPermission, ['bun', bundleEntrypoint, '--target', 'macos']))
        .rejects.toThrow(/permissionOrigins\.microphone requires 'microphone'/i);

    const undeclaredOrigin = await createFixture();
    await writeFile(path.join(undeclaredOrigin, 'package.json'), JSON.stringify({
        name: 'native-fixture', private: true,
        tachyon: {
            devicePermissions: ['microphone'],
            managedContentOrigins: ['https://chatgpt.com'],
            permissionOrigins: { microphone: ['https://claude.ai'] },
        },
    }, null, 2));
    await expect(runCli(undeclaredOrigin, ['bun', bundleEntrypoint, '--target', 'macos']))
        .rejects.toThrow(/permissionOrigins\.microphone must be contained.+managedContentOrigins/i);
});

timedTest('removed render-mode arguments are rejected by both native commands', { timeout: 30000 }, async () => {
    const cwd = await createFixture();
    await expect(runCli(cwd, ['bun', bundleEntrypoint, '--target', 'android', '--render-mode', 'webview']))
        .rejects.toThrow(/--render-mode has been removed/i);
    await runCli(cwd, ['bun', bundleEntrypoint, '--target', 'android', '--skip-native-host']);
    await expect(runCli(cwd, ['bun', nativeEntrypoint, '--target', 'android', '--render-mode=hybrid']))
        .rejects.toThrow(/--render-mode has been removed/i);
});
