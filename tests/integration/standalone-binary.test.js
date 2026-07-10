// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { access, mkdtemp, readdir, rm } from 'fs/promises';
import path from 'path';
import { tmpdir } from 'os';

const timedTest = /** @type {any} */ (test);
/** @type {string[]} */
const tempDirs = [];
/** @type {Array<import('bun').Subprocess<'ignore' | 'pipe', 'ignore' | 'pipe', 'ignore' | 'pipe'>>} */
const processes = [];

/** @param {string} filePath */
async function exists(filePath) {
    try {
        await access(filePath);
        return true;
    }
    catch {
        return false;
    }
}

/**
 * @param {string[]} cmd
 * @param {{ cwd?: string, env?: Record<string, string | undefined>, timeoutMs?: number }} [options]
 */
async function run(cmd, options = {}) {
    const proc = Bun.spawn(cmd, {
        cwd: options.cwd,
        env: options.env,
        stdout: 'pipe',
        stderr: 'pipe',
    });
    const timeoutMs = options.timeoutMs ?? 60000;
    const timeout = new Promise((_, reject) => {
        setTimeout(() => reject(new Error(`Command timed out: ${cmd.join(' ')}`)), timeoutMs);
    });
    const [stdout, stderr, exitCode] = /** @type {Promise<[string, string, number]>} */ (await Promise.race([
        Promise.all([
            new Response(proc.stdout).text(),
            new Response(proc.stderr).text(),
            proc.exited,
        ]),
        timeout,
    ]));
    if (exitCode !== 0) {
        throw new Error(`Command failed (${exitCode}): ${cmd.join(' ')}\n${stdout}\n${stderr}`);
    }
}

/**
 * @param {string} url
 * @param {number} timeoutMs
 */
async function waitForHttp(url, timeoutMs = 15000) {
    const deadline = Date.now() + timeoutMs;
    let lastError = null;
    while (Date.now() < deadline) {
        try {
            const response = await fetch(url);
            if (response.ok)
                return response;
            lastError = new Error(`HTTP ${response.status}`);
        }
        catch (error) {
            lastError = error;
        }
        await Bun.sleep(150);
    }
    throw lastError instanceof Error ? lastError : new Error(`Timed out waiting for ${url}`);
}

afterEach(async () => {
    for (const proc of processes.splice(0)) {
        proc.kill();
        await proc.exited.catch(() => undefined);
    }
    for (const dir of tempDirs.splice(0)) {
        await rm(dir, { recursive: true, force: true });
    }
});

timedTest('compiled ty binary can init, bundle, and serve a client app', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-standalone-'));
    tempDirs.push(root);
    const binaryPath = path.join(root, process.platform === 'win32' ? 'ty.exe' : 'ty');

    await run(['bun', 'build', '--compile', path.join(process.cwd(), 'src/cli/index.js'), '--outfile', binaryPath], {
        timeoutMs: 120000,
    });
    await run([binaryPath, 'init', 'smoke'], { cwd: root });

    const appRoot = path.join(root, 'smoke');
    const cacheRoot = path.join(root, 'cache');
    const cacheEnv = { ...process.env, TACHYON_CACHE_DIR: cacheRoot };
    await run([binaryPath, 'bundle', '--target', 'web'], { cwd: appRoot, env: cacheEnv, timeoutMs: 120000 });
    expect(await exists(path.join(appRoot, 'dist', 'web', 'index.html'))).toBe(true);
    expect(await exists(path.join(appRoot, 'dist', 'web', 'spa-renderer.js'))).toBe(true);
    const cacheEntries = await readdir(path.join(cacheRoot, 'runtime'), { withFileTypes: true });
    expect(cacheEntries.some((entry) => entry.isDirectory())).toBe(true);

    await run([binaryPath, 'cache', 'clean'], { cwd: appRoot, env: cacheEnv });
    expect(await exists(path.join(cacheRoot, 'runtime'))).toBe(false);

    const port = String(18880 + Math.floor(Math.random() * 500));
    const server = Bun.spawn([binaryPath, 'serve'], {
        cwd: appRoot,
        env: {
            ...process.env,
            TACHYON_CACHE_DIR: cacheRoot,
            YON_HOST: '127.0.0.1',
            YON_PORT: port,
        },
        stdout: 'pipe',
        stderr: 'pipe',
    });
    processes.push(server);

    const handlerResponse = await waitForHttp(`http://127.0.0.1:${port}/`);
    expect(await handlerResponse.json()).toEqual({ ok: true, framework: 'Tachyon' });

    const runtimeResponse = await waitForHttp(`http://127.0.0.1:${port}/spa-renderer.js`);
    expect(runtimeResponse.headers.get('content-type')).toContain('text/javascript');
    const runtimeSource = await runtimeResponse.text();
    expect(runtimeSource).toContain('window.__tc_rerender');
    expect(runtimeSource).not.toContain('__TACHYON_ASSET_PREFIX__');
    expect(runtimeSource).not.toContain('__tachyonPlaceholder');
    expect(runtimeSource).not.toContain('__tachyonShellPlaceholder');
    expect(await exists(path.join(cacheRoot, 'runtime'))).toBe(true);
}, 180000);
