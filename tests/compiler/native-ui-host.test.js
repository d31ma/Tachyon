// @ts-check
import { afterAll, expect, test } from 'bun:test';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import { generateNativeHost } from '../../src/compiler/native/index.js';

/** @type {string[]} */
const tempDirs = [];
afterAll(async () => {
    await Promise.all(tempDirs.splice(0).map((directory) => rm(directory, { recursive: true, force: true })));
});

async function fixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-native-ui-host-'));
    tempDirs.push(root);
    const assets = path.join(root, 'assets');
    await mkdir(assets, { recursive: true });
    await writeFile(path.join(assets, 'index.html'), '<main><h1>Fallback</h1></main>');
    await writeFile(path.join(assets, 'tachyon.native-controller.js'), 'globalThis.__tachyonNativeUI = {};');
    await writeFile(path.join(assets, 'tachyon.native-ui.json'), JSON.stringify({
        schemaVersion: 1,
        renderMode: 'native',
        entryRoute: '/',
        controller: 'tachyon.native-controller.js',
        routes: [{
            schemaVersion: 1,
            route: '/',
            root: { kind: 'element', tag: 'main', attributes: {}, style: {}, events: {}, children: [] },
        }],
    }));
    return { root, assets };
}

const expectations = {
    macos: { file: ['Sources', 'TachyonApp.swift'], include: ['import SwiftUI', 'import JavaScriptCore', 'TachyonNativeNodeView', 'controller?.call("dispatch"'], exclude: ['import WebKit', 'WKWebView'] },
    ios: { file: ['Sources', 'TachyonNativeView.swift'], include: ['import SwiftUI', 'import JavaScriptCore', 'TachyonNativeNodeView', 'controller?.call("dispatch"'], exclude: ['import WebKit', 'WKWebView'] },
    android: { file: ['app', 'src', 'main', 'java', 'ma', 'del', 'tachyon', 'fixture', 'MainActivity.kt'], include: ['androidx.activity.compose.setContent', 'TachyonNativeNode', 'QuickJs.create()', 'controller.dispatch'], exclude: ['android.webkit', 'WebView'] },
    windows: { file: ['src', 'main.cpp'], include: ['<windows.h>', 'Microsoft.UI.Xaml.Controls.Primitives', 'Windows.Foundation.Collections', 'TachyonNativeNode', 'tachyon_ui_controller_dispatch'], exclude: ['WebView2', 'ICoreWebView2'] },
    linux: { file: ['src', 'main.c'], include: ['<gtk/gtk.h>', 'TachyonNativeNode', 'tachyon_ui_controller_dispatch'], exclude: ['webkit2', 'WebKitWebView'] },
};

for (const [target, contract] of Object.entries(expectations)) {
    test(`${target} native render mode emits its platform UI toolkit without a WebView shell`, async () => {
        const { root, assets } = await fixture();
        const outputRoot = path.join(root, target);
        await generateNativeHost({
            target,
            assetRoot: assets,
            outputRoot,
            appName: 'Fixture',
            appId: 'ma.del.tachyon.fixture',
            renderMode: 'native',
        });
        const source = await readFile(path.join(outputRoot, ...contract.file), 'utf8');
        for (const value of contract.include) expect(source).toContain(value);
        for (const value of contract.exclude) expect(source).not.toContain(value);
        if (target === 'macos' || target === 'ios') {
            expect(source).toContain('VStack(alignment: .leading, spacing: 0)');
            expect(source).toContain('ScrollView(.vertical, showsIndicators: false)');
            expect(source).toContain('private var routes: [TachyonNativeRoute] = []');
            expect(source).toContain('routes = bundle.routes');
            expect(source).toContain('template.hasPrefix(":") || template == value');
            expect(source).toContain('__tachyonResolveURL');
            expect(source).toContain('globalThis.queueMicrotask');
            expect(source).toContain('globalThis.URLSearchParams');
            expect(source).toContain('NSImage(contentsOf: url)');
            expect(source).not.toContain('}.padding()');
        }
        if (target === 'macos') expect(source).toContain('window.setContentSize(NSSize(width: 1200, height: 800))');
        if (target === 'macos') expect(source).toContain('TachyonNativeNodeView(header, model: model)');
        if (target === 'android') {
            expect(source).not.toContain('.padding(16.dp)');
            expect(source).not.toContain('.padding(4.dp)');
        }
        if (target === 'windows') {
            const cmake = await readFile(path.join(outputRoot, 'CMakeLists.txt'), 'utf8');
            expect(cmake).toContain('VS_PACKAGE_REFERENCES "Microsoft.WindowsAppSDK_1.8.260710003;Microsoft.Windows.CppWinRT_3.0.260715.1"');
            expect(cmake).toContain('VS_GLOBAL_WindowsPackageType "None"');
            expect(cmake).toContain('"$(_FoundationLibFolder)/Microsoft.WindowsAppRuntime.lib"');
            expect(cmake).toContain('"$(_FoundationLibFolder)/Microsoft.WindowsAppRuntime.Bootstrap.lib"');
        }
        const manifest = JSON.parse(await readFile(path.join(outputRoot, 'tachyon.host.json'), 'utf8'));
        expect(manifest).toMatchObject({
            renderMode: 'native',
            nativeUIEntry: `${target === 'ios' ? 'WebBundle' : 'Resources'}/tachyon.native-ui.json`,
        });
        if (target === 'macos' && process.platform === 'darwin') {
            const typecheck = Bun.spawn([
                'swiftc', '-typecheck', '-parse-as-library', path.join(outputRoot, ...contract.file),
            ], { stdout: 'pipe', stderr: 'pipe' });
            const [stderr, exitCode] = await Promise.all([
                new Response(typecheck.stderr).text(), typecheck.exited,
            ]);
            if (exitCode !== 0) throw new Error(stderr);
        }
    });
}

const fallbackExpectations = {
    macos: ['import SwiftUI', 'import WebKit', 'TachyonHybridWebView', 'id="tachyon-boundary">\\(fragment)</div>', '<script defer src="imports.js"></script>', 'removeAttribute("type")', '#tachyon-boundary [w-dropdown].open{position:static!important;inset:auto!important}', 'Node.prototype.appendChild', 'ResizeObserver', 'MutationObserver', 'tachyonBoundarySize', 'tachyonBoundaryNavigate', 'tachyonBoundaryTheme', 'model.open(route)', 'model.setTheme(theme)', 'preferredColorScheme'],
    ios: ['import SwiftUI', 'import WebKit', 'TachyonHybridWebView', 'id="tachyon-boundary">\\(fragment)</div>', '<script defer src="imports.js"></script>', 'removeAttribute("type")', '#tachyon-boundary [w-dropdown].open{position:static!important;inset:auto!important}', 'Node.prototype.appendChild', 'ResizeObserver', 'MutationObserver', 'tachyonBoundarySize', 'tachyonBoundaryNavigate', 'tachyonBoundaryTheme', 'model.open(route)', 'model.setTheme(theme)', 'preferredColorScheme'],
    android: ['androidx.activity.compose.setContent', 'AndroidView', 'android.webkit.WebView', 'WebViewAssetLoader', 'https://appassets.androidplatform.net/assets/', '<script defer src=\\"imports.js\\"></script>', 'removeAttribute(\\"type\\")', '#tachyon-boundary [w-dropdown].open{position:static!important;inset:auto!important}', 'Node.prototype.appendChild', 'ResizeObserver', 'MutationObserver', 'isVerticalScrollBarEnabled = false', 'isHorizontalScrollBarEnabled = false', '@JavascriptInterface fun navigate(route: String)', '@JavascriptInterface fun setTheme(value: String)', 'shouldOverrideUrlLoading', 'removePrefix("/assets")', 'firstOrNull { it.getString("route") == nextRoute }', 'controller.open(nextRoute)', 'darkColorScheme()', 'hybridDocument(html, theme)', 'addJavascriptInterface'],
    windows: ['Microsoft.UI.Xaml.Controls', 'WebView2'],
    linux: ['<gtk/gtk.h>', '<webkit2/webkit2.h>', 'webkit_web_view_load_html'],
};

for (const [target, includes] of Object.entries(fallbackExpectations)) {
    test(`${target} automatically emits support for an isolated WebView boundary`, async () => {
        const { root, assets } = await fixture();
        const bundlePath = path.join(assets, 'tachyon.native-ui.json');
        const bundle = JSON.parse(await readFile(bundlePath, 'utf8'));
        bundle.hasWebViewFallbacks = true;
        bundle.webViewFallbacks = ['company-chart'];
        bundle.routes[0].root.children = [{
            kind: 'webview', tag: 'company-chart', id: 'sales', attributes: {}, style: {}, events: {},
            html: '<company-chart id="sales"></company-chart>', children: [],
        }];
        await writeFile(bundlePath, JSON.stringify(bundle));
        const outputRoot = path.join(root, `${target}-fallback`);
        await generateNativeHost({
            target, assetRoot: assets, outputRoot, appName: 'Fixture', appId: 'ma.del.tachyon.fixture',
        });
        const contract = expectations[target];
        const source = await readFile(path.join(outputRoot, ...contract.file), 'utf8');
        for (const value of includes) expect(source).toContain(value);
        if (target === 'macos' || target === 'ios' || target === 'android')
            expect(source).not.toContain('document.documentElement.scrollHeight');
        if (target === 'macos' || target === 'ios' || target === 'android')
            expect(source).toContain('},true);</script>');
        if (target === 'macos' || target === 'ios')
            expect(source).toContain('#tachyon-boundary{display:flow-root;container-type:inline-size}');
        if (target === 'macos' || target === 'ios')
            expect(source).toContain('#tachyon-boundary w-app-bar{display:block;min-height:56px}');
        const manifest = JSON.parse(await readFile(path.join(outputRoot, 'tachyon.host.json'), 'utf8'));
        expect(manifest).toMatchObject({ renderMode: 'native', hasWebViewFallbacks: true, nativeUIEntry: expect.any(String) });
        if (target === 'macos' && process.platform === 'darwin') {
            const typecheck = Bun.spawn(['swiftc', '-typecheck', '-parse-as-library', path.join(outputRoot, ...contract.file)], {
                stdout: 'pipe', stderr: 'pipe',
            });
            const [stderr, exitCode] = await Promise.all([new Response(typecheck.stderr).text(), typecheck.exited]);
            if (exitCode !== 0) throw new Error(stderr);
        }
    });
}

test('macOS emits isolated managed-content, permission, shortcut, window, and capture adapters', async () => {
    const { root, assets } = await fixture();
    const outputRoot = path.join(root, 'macos-capabilities');
    await generateNativeHost({
        target: 'macos', assetRoot: assets, outputRoot, appName: 'Fixture', appId: 'ma.del.tachyon.fixture',
        devicePermissions: ['microphone', 'screenCapture'],
        managedContentOrigins: ['https://chatgpt.com'],
        permissionOrigins: { microphone: ['https://chatgpt.com'] },
    });
    const sourcePath = path.join(outputRoot, 'Sources', 'TachyonApp.swift');
    const source = await readFile(sourcePath, 'utf8');
    expect(source).toContain('class TachyonManagedSurface');
    expect(source).toContain('requestMediaCapturePermissionFor');
    expect(source).toContain('window.captureProtection');
    expect(source).toContain('shortcuts.register');
    expect(source).toContain('screenCapture.captureWindow');
    expect(source).toContain('__tachyonNativeHostCall');
    expect(source).not.toContain('configuration.userContentController.addUserScript');
    const manifest = JSON.parse(await readFile(path.join(outputRoot, 'tachyon.host.json'), 'utf8'));
    expect(manifest.hostCapabilities).toEqual(expect.arrayContaining([
        'contentSurface.open', 'screenCapture.captureWindow', 'shortcuts.register', 'window.opacity',
    ]));
    if (process.platform === 'darwin') {
        const typecheck = Bun.spawn(['swiftc', '-typecheck', '-parse-as-library', sourcePath], { stdout: 'pipe', stderr: 'pipe' });
        const [stderr, exitCode] = await Promise.all([new Response(typecheck.stderr).text(), typecheck.exited]);
        if (exitCode !== 0) throw new Error(stderr);
        const build = Bun.spawn(['sh', 'build.sh'], { cwd: outputRoot, stdout: 'pipe', stderr: 'pipe' });
        const [buildOutput, buildError, buildExitCode] = await Promise.all([
            new Response(build.stdout).text(), new Response(build.stderr).text(), build.exited,
        ]);
        if (buildExitCode !== 0) throw new Error(`${buildOutput}\n${buildError}`);
        expect(await readFile(path.join(outputRoot, 'build', 'Fixture.app', 'Contents', 'Info.plist'), 'utf8'))
            .toContain('NSMicrophoneUsageDescription');
    }
});

for (const target of ['windows', 'linux']) {
    test(`${target} emits only its concrete desktop capability adapters`, async () => {
        const { root, assets } = await fixture();
        const outputRoot = path.join(root, `${target}-capabilities`);
        await generateNativeHost({
            target, assetRoot: assets, outputRoot, appName: 'Fixture', appId: 'ma.del.tachyon.fixture',
            devicePermissions: target === 'windows' ? ['microphone', 'screenCapture'] : ['microphone'],
            managedContentOrigins: ['https://chatgpt.com'],
            permissionOrigins: { microphone: ['https://chatgpt.com'] },
        });
        const contract = expectations[target];
        const source = await readFile(path.join(outputRoot, ...contract.file), 'utf8');
        expect(source).toContain('contentSurface.open');
        expect(source).toContain('https://chatgpt.com');
        expect(source).not.toContain('AddScriptToExecuteOnDocumentCreated');
        const manifest = JSON.parse(await readFile(path.join(outputRoot, 'tachyon.host.json'), 'utf8'));
        expect(manifest.hostCapabilities).toContain('contentSurface.open');
        expect(manifest.managedContentPolicy).toMatchObject({
            allowedOrigins: ['https://chatgpt.com'], permissions: 'declared-origin-only',
        });
        if (target === 'windows') {
            expect(source).toContain('HandleNativeCapability');
            expect(source).toContain('RegisterHotKey');
            expect(source).toContain('SetWindowDisplayAffinity');
            expect(source).toContain('screenCapture.captureWindow');
            expect(source).toContain('CapabilityEnabled(std::wstring(payload.GetNamedString');
            expect(source).toContain('auto url = std::wstring(payload.GetNamedString');
            expect(source).toContain('ParseAccelerator(std::wstring(accelerator))');
            expect(source).toContain('EnsureCoreWebView2Async(environment, options)');
            expect(source).toContain('options.IsInPrivateModeEnabled(!persistent)');
            expect(source).not.toContain('CoreWebView2CreationProperties');
            expect(manifest.hostCapabilities).toEqual(expect.arrayContaining([
                'shortcuts.register', 'window.captureProtection', 'screenCapture.captureWindow',
            ]));
        } else {
            expect(source).toContain('handle_native_capability');
            expect(source).toContain('webkit_permission_request_allow');
            expect(source).toContain('success_json("{\\"written\\":true}", result_json)');
            expect(source).not.toContain('success_json("{"written":true}", result_json)');
            expect(source).not.toContain('shortcuts.register');
            expect(source).not.toContain('screenCapture.captureWindow');
            expect(manifest.hostCapabilities).not.toContain('screenCapture.captureWindow');
            if (process.platform === 'linux' && process.env.TACHYON_NATIVE_COMPILE_SMOKE === '1') {
                const build = Bun.spawn(['sh', 'build.sh'], { cwd: outputRoot, stdout: 'pipe', stderr: 'pipe' });
                const [buildOutput, buildError, buildExitCode] = await Promise.all([
                    new Response(build.stdout).text(), new Response(build.stderr).text(), build.exited,
                ]);
                if (buildExitCode !== 0) throw new Error(`${buildOutput}\n${buildError}`);
                expect(await Bun.file(path.join(outputRoot, 'build', 'Fixture')).exists()).toBe(true);
            }
        }
    });
}
