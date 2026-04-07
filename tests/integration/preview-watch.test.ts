import { afterEach, expect, test } from 'bun:test'
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
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
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-preview-watch-'))
    tempDirs.push(root)

    await mkdir(path.join(root, 'routes'), { recursive: true })

    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-preview-watch-fixture',
        private: true
    }, null, 2))

    await writeFile(path.join(root, 'main.js'), 'console.log("fixture boot")\n')
    await writeFile(path.join(root, 'routes', 'LAYOUT'), `<html><body><slot /></body></html>`)
    await writeFile(path.join(root, 'routes', 'HTML'), `<main><h1>Alpha</h1></main>`)

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

test('tach.preview --watch serves initial bundle and rebuilds on HTML route changes', { timeout: 20000 }, async () => {
    const root = await createExampleApp()
    const port = 32000 + Math.floor(Math.random() * 2000)
    const preview = Bun.spawn(
        ['bun', path.join(import.meta.dir, '../../src/cli/preview.ts'), '--watch'],
        {
            cwd: root,
            env: { ...process.env, PORT: String(port), HOST: '127.0.0.1' },
            stdout: 'ignore',
            stderr: 'ignore'
        }
    )
    processes.push(preview)

    await waitFor(async () => {
        try {
            const res = await fetch(`http://127.0.0.1:${port}/`)
            return res.ok && (await res.text()).includes('Alpha')
        } catch {
            return false
        }
    })

    await writeFile(path.join(root, 'routes', 'HTML'), `<main><h1>Beta</h1></main>`)

    await waitFor(async () => {
        const res = await fetch(`http://127.0.0.1:${port}/`)
        return res.ok && (await res.text()).includes('Beta')
    })

    const distHtml = await Bun.file(path.join(root, 'dist', 'index.html')).text()
    expect(distHtml).toContain('Beta')
})
