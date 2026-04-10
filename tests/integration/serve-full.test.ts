import { afterEach, expect, test } from 'bun:test'
import { chmod, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import net from 'node:net'
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
const request = await Bun.stdin.json()
Bun.stdout.write(JSON.stringify({ ok: true, fixture: 'api', requestId: request.context.requestId }))
`)
    await chmod(getRoutePath, 0o755)

    return root
}

async function waitFor(check: () => Promise<boolean>, timeoutMs = 15000, intervalMs = 200) {
    const start = Date.now()

    while (Date.now() - start < timeoutMs) {
        if (await check()) return true
        await Bun.sleep(intervalMs)
    }

    return false
}

async function getFreePort() {
    return await new Promise<number>((resolve, reject) => {
        const server = net.createServer()

        server.once('error', reject)
        server.listen(0, '127.0.0.1', () => {
            const address = server.address()
            if (!address || typeof address === 'string') {
                server.close()
                reject(new Error('Failed to resolve an ephemeral port'))
                return
            }

            const { port } = address
            server.close((error) => {
                if (error) reject(error)
                else resolve(port)
            })
        })
    })
}

test('tach.serve --full serves frontend and backend responses from the same port', { timeout: 40000 }, async () => {
    const root = await createExampleApp()
    const port = await getFreePort()
    const env = {
        ...process.env,
        PORT: String(port),
        HOST: '127.0.0.1',
        DEV: 'true',
    } as Record<string, string>

    delete env.BASIC_AUTH
    delete env.VALIDATE

    let lastSnapshot = 'no attempts yet'

    const proc = Bun.spawn(
        ['bun', path.join(import.meta.dir, '../../src/cli/serve.ts'), '--full'],
        {
            cwd: root,
            env,
            stdout: 'pipe',
            stderr: 'pipe'
        }
    )

    processes.push(proc)

    const ok = await waitFor(async () => {
        try {
            const requestId = 'serve-full-test-request-id'
            const [frontendRes, apiRes] = await Promise.all([
                fetch(`http://127.0.0.1:${port}/`, {
                    headers: { accept: 'text/html' }
                }),
                fetch(`http://127.0.0.1:${port}/`, {
                    headers: {
                        accept: 'application/json',
                        'x-request-id': requestId,
                    }
                })
            ])
            const apiBody = await apiRes.json().catch(() => null) as { fixture?: string, requestId?: string } | null

            const htmlOk = frontendRes.ok
                && (await frontendRes.text()).includes('Fixture Home')
            const apiOk = apiRes.ok
                && apiBody?.fixture === 'api'
                && apiBody?.requestId === requestId
                && apiRes.headers.get('x-request-id') === requestId

            lastSnapshot = JSON.stringify({
                htmlStatus: frontendRes.status,
                apiStatus: apiRes.status,
                apiRequestId: apiRes.headers.get('x-request-id'),
                apiBody,
                htmlOk,
                apiOk,
            })

            return htmlOk && apiOk
        } catch {
            lastSnapshot = 'request connection failed'
            return false
        }
    }, 30000)

    if (!ok) {
        proc.kill()
        await proc.exited.catch(() => {})
        const stdout = await new Response(proc.stdout).text()
        const stderr = await new Response(proc.stderr).text()
        throw new Error(
            `serve --full did not become ready. last=${lastSnapshot}\nSTDOUT:\n${stdout}\nSTDERR:\n${stderr}`
        )
    }

    expect(proc.exitCode).toBeNull()
})
