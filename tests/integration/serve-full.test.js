// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { chmod, mkdir, mkdtemp, rm, writeFile } from 'fs/promises';
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
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});
/**
 * @param {{ frontend?: boolean, backend?: boolean }} [options]
 */
async function createExampleApp(options = {}) {
    const { frontend = true, backend = true } = options;
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-serve-full-'));
    tempDirs.push(root);
    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-serve-full-fixture',
        private: true
    }, null, 2));
    if (frontend) {
        await mkdir(path.join(root, 'browser', 'pages'), { recursive: true });
        await mkdir(path.join(root, 'browser', 'shared', 'scripts'), { recursive: true });
        await writeFile(path.join(root, 'browser', 'shared', 'scripts', 'imports.js'), `import { bootMessage } from "./import-support.js";
import "./imports.css";

console.log(bootMessage);
document.documentElement.dataset.boot = bootMessage;
`);
        await writeFile(path.join(root, 'browser', 'shared', 'scripts', 'import-support.js'), `export const bootMessage = "fixture-boot";\n`);
        await writeFile(path.join(root, 'browser', 'shared', 'scripts', 'imports.css'), `body { background: rgb(12, 34, 56); }\n`);
        await writeFile(path.join(root, 'browser', 'pages', 'index.html'), `<main><h1>Fixture Home</h1></main>`);
    }
    if (backend) {
        await mkdir(path.join(root, 'server', 'routes', 'api'), { recursive: true });
        const getRoutePath = path.join(root, 'server', 'routes', 'api', 'GET.js');
        await writeFile(getRoutePath, `export async function handler(request) {
  return { ok: true, fixture: 'api', requestId: request.context.requestId }
}
`);
        await chmod(getRoutePath, 0o755);
    }
    return root;
}
/** @param {() => boolean | Promise<boolean>} check */
async function waitFor(check, timeoutMs = 15000, intervalMs = 200) {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
        if (await check())
            return true;
        await Bun.sleep(intervalMs);
    }
    return false;
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
timedTest('yon.serve serves frontend and backend responses when browser and server folders exist', { timeout: 40000 }, async () => {
    const root = await createExampleApp();
    const port = await getFreePort();
    /** @type {NodeJS.ProcessEnv} */
    const env = {
        ...process.env,
        YON_PORT: String(port),
        YON_HOST: '127.0.0.1',
        YON_DEV: 'true',
    };
    delete env.YON_BASIC_AUTH;
    delete env.YON_BASIC_AUTH_HASH;
    delete env.YON_VALIDATE;
    let lastSnapshot = 'no attempts yet';
    const proc = Bun.spawn(['bun', path.join(import.meta.dir, '../../src/cli/serve.js')], {
        cwd: root,
        env,
        stdout: 'pipe',
        stderr: 'pipe'
    });
    processes.push(proc);
    const ok = await waitFor(async () => {
        try {
            const requestId = 'serve-full-test-request-id';
            const [frontendRes, apiRes, mainRes, cssRes] = await Promise.all([
                fetch(`http://127.0.0.1:${port}/`, {
                    headers: {
                        'sec-fetch-dest': 'document',
                        'sec-fetch-mode': 'navigate',
                    }
                }),
                fetch(`http://127.0.0.1:${port}/api`, {
                    headers: {
                        accept: 'application/json',
                        'x-request-id': requestId,
                    }
                }),
                fetch(`http://127.0.0.1:${port}/imports.js`),
                fetch(`http://127.0.0.1:${port}/imports.css`),
            ]);
            const apiBody = await apiRes.json().catch(() => null);
            const frontendBody = await frontendRes.text();
            const mainBody = await mainRes.text();
            const cssBody = await cssRes.text();
            const htmlOk = frontendRes.ok
                && frontendBody.includes('Fixture Home')
                && frontendBody.includes('/imports.css')
                && frontendBody.includes('type="module" src="/spa-renderer.js"')
                && frontendBody.includes('type="module" src="/imports.js"');
            const apiOk = apiRes.ok
                && apiBody?.fixture === 'api'
                && apiBody?.requestId === requestId
                && apiRes.headers.get('x-request-id') === requestId;
            const mainOk = mainRes.ok
                && mainRes.headers.get('content-type')?.includes('javascript')
                && mainBody.includes('fixture-boot');
            const cssOk = cssRes.ok
                && cssRes.headers.get('content-type')?.includes('text/css')
                && cssBody.includes('background:#0c2238');
            lastSnapshot = JSON.stringify({
                htmlStatus: frontendRes.status,
                apiStatus: apiRes.status,
                mainStatus: mainRes.status,
                cssStatus: cssRes.status,
                apiRequestId: apiRes.headers.get('x-request-id'),
                apiBody,
                htmlOk,
                apiOk,
                mainOk,
                cssOk,
            });
            return Boolean(htmlOk && apiOk && mainOk && cssOk);
        }
        catch {
            lastSnapshot = 'request connection failed';
            return false;
        }
    }, 30000);
    if (!ok) {
        proc.kill();
        await proc.exited.catch(() => { });
        const stdout = await new Response(proc.stdout).text();
        const stderr = await new Response(proc.stderr).text();
        throw new Error(`serve did not become ready. last=${lastSnapshot}\nSTDOUT:\n${stdout}\nSTDERR:\n${stderr}`);
    }
    expect(proc.exitCode).toBeNull();
});
timedTest('yon.serve serves frontend only when only the browser folder exists', { timeout: 40000 }, async () => {
    const root = await createExampleApp({ backend: false });
    const port = await getFreePort();
    /** @type {NodeJS.ProcessEnv} */
    const env = {
        ...process.env,
        YON_PORT: String(port),
        YON_HOST: '127.0.0.1',
        YON_DEV: 'true',
    };
    delete env.YON_BASIC_AUTH;
    delete env.YON_BASIC_AUTH_HASH;
    delete env.YON_VALIDATE;
    let lastSnapshot = 'no attempts yet';
    const proc = Bun.spawn(['bun', path.join(import.meta.dir, '../../src/cli/serve.js')], {
        cwd: root,
        env,
        stdout: 'pipe',
        stderr: 'pipe'
    });
    processes.push(proc);
    const ok = await waitFor(async () => {
        try {
            const [frontendRes, apiRes] = await Promise.all([
                fetch(`http://127.0.0.1:${port}/`, {
                    headers: {
                        'sec-fetch-dest': 'document',
                        'sec-fetch-mode': 'navigate',
                    }
                }),
                fetch(`http://127.0.0.1:${port}/api`, {
                    headers: {
                        accept: 'application/json',
                    }
                }),
            ]);
            const body = await frontendRes.text();
            lastSnapshot = JSON.stringify({
                frontendStatus: frontendRes.status,
                apiStatus: apiRes.status,
                hasSpaRenderer: body.includes('/spa-renderer.js'),
                hasMainStylesheet: body.includes('/imports.css'),
            });
            return frontendRes.ok
                && body.includes('type="module" src="/spa-renderer.js"')
                && body.includes('type="module" src="/imports.js"')
                && body.includes('/imports.css')
                && apiRes.status === 404;
        }
        catch {
            lastSnapshot = 'request connection failed';
            return false;
        }
    }, 30000);
    if (!ok) {
        proc.kill();
        await proc.exited.catch(() => { });
        const stdout = await new Response(proc.stdout).text();
        const stderr = await new Response(proc.stderr).text();
        throw new Error(`serve did not return frontend-only output. last=${lastSnapshot}\nSTDOUT:\n${stdout}\nSTDERR:\n${stderr}`);
    }
    expect(proc.exitCode).toBeNull();
});
timedTest('yon.serve serves backend only when only the server folder exists', { timeout: 40000 }, async () => {
    const root = await createExampleApp({ frontend: false });
    const port = await getFreePort();
    /** @type {NodeJS.ProcessEnv} */
    const env = {
        ...process.env,
        YON_PORT: String(port),
        YON_HOST: '127.0.0.1',
        YON_DEV: 'true',
    };
    delete env.YON_BASIC_AUTH;
    delete env.YON_BASIC_AUTH_HASH;
    delete env.YON_VALIDATE;
    let lastSnapshot = 'no attempts yet';
    const proc = Bun.spawn(['bun', path.join(import.meta.dir, '../../src/cli/serve.js')], {
        cwd: root,
        env,
        stdout: 'pipe',
        stderr: 'pipe'
    });
    processes.push(proc);
    const ok = await waitFor(async () => {
        try {
            const requestId = 'serve-backend-only-request-id';
            const [apiRes, frontendRes] = await Promise.all([
                fetch(`http://127.0.0.1:${port}/api`, {
                    headers: {
                        accept: 'application/json',
                        'x-request-id': requestId,
                    }
                }),
                fetch(`http://127.0.0.1:${port}/`, {
                    headers: {
                        'sec-fetch-dest': 'document',
                        'sec-fetch-mode': 'navigate',
                    }
                }),
            ]);
            const apiBody = await apiRes.json().catch(() => null);
            const frontendBody = await frontendRes.text();
            const apiOk = apiRes.ok
                && apiBody?.fixture === 'api'
                && apiBody?.requestId === requestId;
            lastSnapshot = JSON.stringify({
                apiStatus: apiRes.status,
                frontendStatus: frontendRes.status,
                apiBody,
                frontendBody,
            });
            return Boolean(apiOk && frontendRes.status === 404);
        }
        catch {
            lastSnapshot = 'request connection failed';
            return false;
        }
    }, 30000);
    if (!ok) {
        proc.kill();
        await proc.exited.catch(() => { });
        const stdout = await new Response(proc.stdout).text();
        const stderr = await new Response(proc.stderr).text();
        throw new Error(`serve did not return backend-only output. last=${lastSnapshot}\nSTDOUT:\n${stdout}\nSTDERR:\n${stderr}`);
    }
    expect(proc.exitCode).toBeNull();
});
