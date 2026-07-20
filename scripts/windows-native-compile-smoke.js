// @ts-check
import { access, mkdir, mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import { generateNativeHost } from '../src/compiler/native/index.js';

if (process.platform !== 'win32')
    throw new Error('The Windows native compile smoke must run on Windows with MSVC and WebView2 installed.');
if (!process.env.WEBVIEW2_SDK_ROOT)
    throw new Error('WEBVIEW2_SDK_ROOT must point at an unpacked Microsoft.Web.WebView2 NuGet package.');

/** @param {string[]} command @param {string} cwd */
async function run(command, cwd) {
    const processHandle = Bun.spawn(command, {
        cwd,
        env: process.env,
        stdout: 'inherit',
        stderr: 'inherit',
    });
    const exitCode = await processHandle.exited;
    if (exitCode !== 0)
        throw new Error(`${command.join(' ')} failed with exit code ${exitCode}`);
}

const root = await mkdtemp(path.join(tmpdir(), 'tachyon-windows-native-smoke-'));
const assetRoot = path.join(root, 'web');
const hostRoot = path.join(root, 'host');
const buildRoot = path.join(root, 'build');

try {
    await mkdir(assetRoot, { recursive: true });
    await writeFile(path.join(assetRoot, 'index.html'), '<!doctype html><html><head></head><body>Windows native smoke</body></html>');
    await writeFile(path.join(assetRoot, 'tachyon.native-controller.js'), 'globalThis.__tachyonNativeUI = {};');
    await writeFile(path.join(assetRoot, 'tachyon.native-ui.json'), JSON.stringify({
        schemaVersion: 1,
        renderMode: 'native',
        entryRoute: '/',
        controller: 'tachyon.native-controller.js',
        hasWebViewFallbacks: false,
        webViewFallbacks: [],
        routes: [{
            schemaVersion: 1,
            route: '/',
            root: {
                kind: 'element',
                tag: 'main',
                attributes: {},
                style: {},
                events: {},
                children: [],
            },
        }],
    }));

    await generateNativeHost({
        target: 'windows',
        assetRoot,
        outputRoot: hostRoot,
        appName: 'TachyonWindowsSmoke',
    });

    await run(['cmake', '-S', hostRoot, '-B', buildRoot, '-A', 'x64'], root);
    await run(['cmake', '--build', buildRoot, '--config', 'Release'], root);
    await access(path.join(buildRoot, 'Release', 'TachyonWindowsSmoke.exe'));
}
finally {
    if (process.env.KEEP_NATIVE_SMOKE !== '1')
        await rm(root, { recursive: true, force: true });
    else
        console.log(`Windows native smoke project retained at ${root}`);
}
