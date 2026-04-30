// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { mkdir, mkdtemp, rm, writeFile } from 'fs/promises';
import net from 'net';
import path from 'path';
import { tmpdir } from 'os';
const timedTest = /** @type {any} */ (test);
/** @type {string[]} */
const tempDirs = [];
/** @type {Bun.Subprocess<any, any, any>[]} */
const processes = [];
afterEach(async () => {
    for (const proc of processes.splice(0)) {
        proc.kill();
        await proc.exited.catch(() => { });
    }
    await Promise.all(tempDirs.splice(0).map((dir) => removeTempDir(dir)));
});

/** @param {string} dir */
async function removeTempDir(dir) {
    for (let attempt = 1; attempt <= 8; attempt++) {
        try {
            await rm(dir, { recursive: true, force: true });
            return;
        }
        catch (error) {
            const code = /** @type {{ code?: string }} */ (error).code;
            if (!['EBUSY', 'ENOTEMPTY', 'EPERM'].includes(String(code)) || attempt === 8) {
                throw error;
            }
            await Bun.sleep(attempt * 100);
        }
    }
}

async function createExampleApp() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-preview-watch-'));
    tempDirs.push(root);
    await mkdir(path.join(root, 'routes'), { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-preview-watch-fixture',
        private: true
    }, null, 2));
    await writeFile(path.join(root, 'imports.js'), 'console.log("fixture boot")\n');
    await writeFile(path.join(root, 'routes', 'index.html'), `<main><h1>Alpha</h1></main>`);
    return root;
}
/** @param {() => boolean | Promise<boolean>} check */
async function waitFor(check, timeoutMs = 15000, intervalMs = 200) {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
        if (await check())
            return;
        await Bun.sleep(intervalMs);
    }
    throw new Error(`Condition not met within ${timeoutMs}ms`);
}
async function getFreePort() {
    return await new Promise((resolve, reject) => {
        const server = net.createServer();
        server.once('error', reject);
        server.listen(0, '127.0.0.1', () => {
            const address = server.address();
            if (!address || typeof address === 'string') {
                server.close();
                reject(new Error('Failed to resolve an ephemeral port'));
                return;
            }
            const { port } = address;
            server.close((error) => {
                if (error)
                    reject(error);
                else
                    resolve(port);
            });
        });
    });
}
timedTest('tac.preview --watch serves initial bundle and rebuilds on HTML route changes', { timeout: 20000 }, async () => {
    const root = await createExampleApp();
    const port = await getFreePort();
    const preview = Bun.spawn(['bun', path.join(import.meta.dir, '../../src/cli/preview.js'), '--watch'], {
        cwd: root,
        env: { ...process.env, YON_PORT: String(port), YON_HOST: '127.0.0.1' },
        stdout: 'ignore',
        stderr: 'ignore'
    });
    processes.push(preview);
    await waitFor(async () => {
        try {
            const res = await fetch(`http://127.0.0.1:${port}/`);
            return res.ok && (await res.text()).includes('Alpha');
        }
        catch {
            return false;
        }
    });
    await writeFile(path.join(root, 'routes', 'index.html'), `<main><h1>Beta</h1></main>`);
    await waitFor(async () => {
        const res = await fetch(`http://127.0.0.1:${port}/`);
        return res.ok && (await res.text()).includes('Beta');
    });
    const distHtml = await Bun.file(path.join(root, 'dist', 'index.html')).text();
    expect(distHtml).toContain('Beta');
});
