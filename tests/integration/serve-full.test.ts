import { afterEach, expect, test } from 'bun:test'
import { chmod, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { tmpdir } from 'node:os'

const tempDirs: string[] = []
const processes: Bun.Subprocess[] = []

afterEach(async () => {
    for (const proc of processes.splice(0)) {
        proc.kill()
        await proc.exited.catch(() => {})
    }

    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })))
})

async function createExampleApp() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-serve-full-'))
    tempDirs.push(root)

    await mkdir(path.join(root, 'routes'), { recursive: true })

    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-serve-full-fixture',
        private: true
    }, null, 2))

    await writeFile(path.join(root, 'main.js'), 'console.log("fixture boot")\n')
    await writeFile(path.join(root, 'routes', 'LAYOUT'), `<html><body><slot /></body></html>`)
    await writeFile(path.join(root, 'routes', 'HTML'), `<main><h1>Fixture Home</h1></main>`)
    const getRoutePath = path.join(root, 'routes', 'GET')
    await writeFile(getRoutePath, `#!/usr/bin/env bun
Bun.stdout.write(JSON.stringify({ ok: true, fixture: 'api' }))
`)
    await chmod(getRoutePath, 0o755)

    return root
}

async function waitFor(check: () => Promise<boolean>, timeoutMs = 15000, intervalMs = 200) {
    const start = Date.now()

    while (Date.now() - start < timeoutMs) {
        if (await check()) return
        await Bun.sleep(intervalMs)
    }

    throw new Error(`Condition not met within ${timeoutMs}ms`)
}

test('tach.serve --full starts the app server and preview server together', { timeout: 20000 }, async () => {
    const root = await createExampleApp()
    const apiPort = 34000 + Math.floor(Math.random() * 1000)
    const previewPort = apiPort + 1000

    const proc = Bun.spawn(
        ['bun', path.join(import.meta.dir, '../../src/cli/serve.ts'), '--full'],
        {
            cwd: root,
            env: {
                ...process.env,
                PORT: String(apiPort),
                PREVIEW_PORT: String(previewPort),
                HOST: '127.0.0.1',
                PREVIEW_HOST: '127.0.0.1',
                DEV: 'true',
            },
            stdout: 'ignore',
            stderr: 'ignore'
        }
    )

    processes.push(proc)

    await waitFor(async () => {
        try {
            const [apiRes, previewRes] = await Promise.all([
                fetch(`http://127.0.0.1:${apiPort}/routes.json`),
                fetch(`http://127.0.0.1:${previewPort}/`)
            ])

            return apiRes.ok
                && (await apiRes.text()).includes('"/"')
                && previewRes.ok
                && (await previewRes.text()).includes('Fixture Home')
        } catch {
            return false
        }
    })

    expect(proc.exitCode).toBeNull()
})
