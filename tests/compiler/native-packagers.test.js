// @ts-check
import { afterEach, expect, test } from 'bun:test';
import path from 'path';
import { mkdtemp, readFile, rm, writeFile, mkdir } from 'fs/promises';
import { tmpdir } from 'os';
import { hasNativePackager, packageNativeArtifact } from '../../src/compiler/native/packagers.js';
import { generateNativeHost } from '../../src/compiler/native/index.js';

/** @type {string[]} */
const tempDirs = [];

async function makeAssetRoot() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-packager-'));
    tempDirs.push(root);
    const assetRoot = path.join(root, 'web');
    await mkdir(assetRoot, { recursive: true });
    await writeFile(path.join(assetRoot, 'index.html'), '<!doctype html><title>fixture</title>');
    await writeFile(path.join(assetRoot, 'tachyon.native-controller.js'), 'globalThis.__tachyonNativeUI = {};');
    await writeFile(path.join(assetRoot, 'tachyon.native-ui.json'), JSON.stringify({
        schemaVersion: 1,
        renderMode: 'native',
        entryRoute: '/',
        controller: 'tachyon.native-controller.js',
        hasWebViewFallbacks: false,
        webViewFallbacks: [],
        routes: [{ schemaVersion: 1, route: '/', root: { kind: 'element', tag: 'main', children: [] } }],
    }));
    return { root, assetRoot };
}

afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

test('every native target has an artifact packager', () => {
    expect(hasNativePackager('android')).toBe(true);
    expect(hasNativePackager('ios')).toBe(true);
    expect(hasNativePackager('macos')).toBe(true);
    expect(hasNativePackager('linux')).toBe(true);
    expect(hasNativePackager('windows')).toBe(true);
    expect(hasNativePackager('web')).toBe(false);
});

test('unsupported targets skip with a reason instead of throwing', async () => {
    const result = await packageNativeArtifact({ target: 'web', projectRoot: '/tmp/nowhere', appName: 'X', version: '1.0.0' });
    expect('skipped' in result && result.skipped).toContain("No artifact packager for target 'web'");
});

test('cross-platform desktop targets skip off-platform with guidance', async () => {
    if (process.platform !== 'linux') {
        const linux = await packageNativeArtifact({ target: 'linux', projectRoot: '/tmp/nowhere', appName: 'X', version: '1.0.0' });
        expect('skipped' in linux && linux.skipped).toContain('only be built on Linux');
    }
    if (process.platform !== 'win32') {
        const windows = await packageNativeArtifact({ target: 'windows', projectRoot: '/tmp/nowhere', appName: 'X', version: '1.0.0' });
        expect('skipped' in windows && windows.skipped).toContain('only be built on Windows');
    }
});

test('generated android host carries the release signing fallback', async () => {
    const { root, assetRoot } = await makeAssetRoot();
    const outputRoot = path.join(root, 'android');
    await generateNativeHost({ target: 'android', assetRoot, outputRoot, appName: 'PackagerFixture', version: '2.3.4' });

    const gradle = await readFile(path.join(outputRoot, 'app', 'build.gradle.kts'), 'utf8');
    expect(gradle).toContain('TAC_ANDROID_KEYSTORE');
    expect(gradle).toContain('signingConfigs.getByName("debug")');
    expect(gradle).toContain('versionName = "2.3.4"');

    const activity = await readFile(path.join(
        outputRoot, 'app', 'src', 'main', 'java', 'ma', 'del', 'tachyon', 'packagerfixture', 'MainActivity.kt',
    ), 'utf8');
    expect(activity).toContain('class MainActivity : ComponentActivity()');
    expect(activity).toContain('QuickJs.create()');
});

test('generated ios host ships an xcodegen spec for .ipa export', async () => {
    const { root, assetRoot } = await makeAssetRoot();
    const outputRoot = path.join(root, 'ios');
    await generateNativeHost({ target: 'ios', assetRoot, outputRoot, appName: 'PackagerFixture', version: '2.3.4' });

    const spec = await readFile(path.join(outputRoot, 'project.yml'), 'utf8');
    expect(spec).toContain('name: PackagerFixture');
    expect(spec).toContain('type: application');
    expect(spec).toContain('platform: iOS');
    expect(spec).toContain('CFBundleShortVersionString: "2.3.4"');
    // The web bundle ships as a folder reference (named WebBundle — a
    // top-level "Resources" folder breaks flat iOS bundle detection).
    expect(spec).toContain('- path: WebBundle\n        type: folder');
});
