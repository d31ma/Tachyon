import { afterEach, expect, test } from 'bun:test'
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { tmpdir } from 'node:os'

const tempDirs: string[] = []
const bundleEntrypoint = path.join(process.cwd(), 'src/cli/bundle.ts')

async function createFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-bundle-'))
    tempDirs.push(root)

    await mkdir(path.join(root, 'routes', 'docs'), { recursive: true })

    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-bundle-fixture',
        private: true
    }, null, 2))

    await writeFile(
        path.join(root, 'routes', 'LAYOUT'),
        `<style>.shell { padding: 1rem; }</style><div class="shell"><slot /></div>`
    )

    await writeFile(
        path.join(root, 'routes', 'HTML'),
        `<script>document.title = "Fixture Home"</script><style>.hero { color: tomato; }</style><h1>Fixture Home</h1>`
    )

    await writeFile(
        path.join(root, 'routes', 'docs', 'HTML'),
        `<script>document.title = "Fixture Docs"</script><p>Docs page</p>`
    )

    return root
}

async function decode(stream: ReadableStream<Uint8Array> | null) {
    if (!stream) return ''
    return await new Response(stream).text()
}

afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })))
})

test('tach.bundle prerenders HTML routes into static documents', { timeout: 20000 }, async () => {
    const cwd = await createFixture()

    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe'
    })

    const [stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ])

    expect(exitCode).toBe(0)
    expect(stderr).toBe('')
    expect(stdout).toContain('Built in')

    const home = await readFile(path.join(cwd, 'dist', 'index.html'), 'utf8')
    const docs = await readFile(path.join(cwd, 'dist', 'docs', 'index.html'), 'utf8')

    expect(home).toContain('<title>Fixture Home</title>')
    expect(home).toContain('class="shell"')
    expect(home).toContain('>Fixture Home</h1>')
    expect(home).not.toContain('@scope')
    expect(home).toContain('<script src="/spa-renderer.js" defer></script>')

    expect(docs).toContain('<title>Fixture Docs</title>')
    expect(docs).toContain('>Docs page</p>')
    expect(docs).toContain('class="shell"')
})
