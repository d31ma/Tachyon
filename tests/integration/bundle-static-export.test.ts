import { afterEach, expect, test } from 'bun:test'
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { tmpdir } from 'node:os'
import { pathToFileURL } from 'node:url'

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

async function createTagClassificationFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-tag-classification-'))
    tempDirs.push(root)

    await mkdir(path.join(root, 'routes'), { recursive: true })
    await mkdir(path.join(root, 'components'), { recursive: true })

    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-tag-classification-fixture',
        private: true
    }, null, 2))

    await writeFile(
        path.join(root, 'components', 'hero-card.html'),
        `<article class="hero-card">Tachyon component wins</article>`
    )

    await writeFile(
        path.join(root, 'routes', 'HTML'),
        [
            '<hero-card />',
            '<user-card data-kind="web-component"></user-card>',
            '<mystery>Unknown tag survives with warning</mystery>',
        ].join('')
    )

    return root
}

async function createLoopEventFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-loop-event-'))
    tempDirs.push(root)

    await mkdir(path.join(root, 'routes'), { recursive: true })

    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-loop-event-fixture',
        private: true
    }, null, 2))

    await writeFile(
        path.join(root, 'routes', 'HTML'),
        `<script>
let tasks = [{ text: "One", done: false }];
function toggle(index) {
    tasks = tasks.map((task, i) => i === index ? { ...task, done: !task.done } : task);
}
function status() {
    return tasks[0].done ? "done" : "pending";
}
</script>
<loop :for="let i = 0; i < tasks.length; i++">
  <button @click="toggle(i)">{status()}</button>
</loop>`
    )

    return root
}

async function createEscapingFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-escaping-'))
    tempDirs.push(root)

    await mkdir(path.join(root, 'routes'), { recursive: true })

    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-escaping-fixture',
        private: true
    }, null, 2))

    await writeFile(
        path.join(root, 'routes', 'HTML'),
        `<script>
let message = '<img src=x onerror=alert(1)>';
let title = '" onfocus="alert(1)';
let trusted = '<strong>Trusted raw HTML</strong>';
</script>
<p>{message}</p>
<div :title="title">Hover me</div>
<section>{!trusted}</section>`
    )

    return root
}

async function createGlobalYonFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-global-yon-'))
    tempDirs.push(root)

    await mkdir(path.join(root, 'routes'), { recursive: true })
    await mkdir(path.join(root, 'components'), { recursive: true })

    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-global-yon-fixture',
        private: true
    }, null, 2))

    await writeFile(
        path.join(root, 'components', 'badge.html'),
        `<script>let label = ''</script><strong class="badge">Badge: {label}</strong>`
    )

    await writeFile(
        path.join(root, 'routes', 'HTML'),
        `<script>let title = 'Global Yon'</script><badge :label="title" />`
    )

    return root
}

async function createAsyncEventFixture() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-async-event-'))
    tempDirs.push(root)

    await mkdir(path.join(root, 'routes'), { recursive: true })

    await writeFile(path.join(root, 'package.json'), JSON.stringify({
        name: 'tachyon-async-event-fixture',
        private: true
    }, null, 2))

    await writeFile(
        path.join(root, 'routes', 'HTML'),
        `<script>
let status = 'idle';
async function requestMfa() {
    status = 'loading';
    await Promise.resolve();
    status = 'phone-input';
}
</script>
<button @click="requestMfa()">Continue</button>
<p>{status}</p>`
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

    if (exitCode !== 0) throw new Error(stderr)
    expect(stderr).toBe('')
    expect(stdout).toContain('Bundle completed')

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

test('tach.bundle classifies component, web component, native, and unknown tags by priority', { timeout: 20000 }, async () => {
    const cwd = await createTagClassificationFixture()

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

    if (exitCode !== 0) throw new Error(stderr)
    expect(stdout).toContain('Bundle completed')
    expect(stderr).toContain('Unknown element tag')
    expect(stderr).toContain('mystery')

    const home = await readFile(path.join(cwd, 'dist', 'index.html'), 'utf8')

    expect(home).toContain('Tachyon component wins')
    expect(home).toContain('<user-card')
    expect(home).toContain('data-kind="web-component"')
    expect(home).toContain('<mystery')
    expect(home).toContain('Unknown tag survives with warning')
})

test('loop-scoped event handlers can access loop variables when rerendered', { timeout: 20000 }, async () => {
    const cwd = await createLoopEventFixture()

    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe'
    })

    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ])

    if (exitCode !== 0) throw new Error(stderr)
    expect(stderr).toBe('')

    const pageModulePath = path.join(cwd, 'dist', 'pages', 'HTML.js')
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?loop-event=${Date.now()}`)
    const render = await pageModule.default()
    const initial = await render()
    const buttonId = initial.match(/<button[^>]* id="([^"]+)"/)?.[1]

    expect(buttonId).toBeDefined()
    expect(initial).toContain('pending')

    await render(buttonId)
    const updated = await render()

    expect(updated).toContain('done')
})

test('template interpolation and dynamic attributes are escaped by default', { timeout: 20000 }, async () => {
    const cwd = await createEscapingFixture()

    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe'
    })

    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ])

    if (exitCode !== 0) throw new Error(stderr)
    expect(stderr).toBe('')

    const pageModulePath = path.join(cwd, 'dist', 'pages', 'HTML.js')
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?escaping=${Date.now()}`)
    const render = await pageModule.default()
    const html = await render()

    expect(html).toContain('&lt;img src=x onerror=alert(1)&gt;')
    expect(html).not.toContain('<img src=x onerror=alert(1)>')
    expect(html).toContain('title="&quot; onfocus=&quot;alert(1)"')
    expect(html).not.toContain('title="" onfocus="alert(1)"')
    expect(html).toContain('<strong>Trusted raw HTML</strong>')
})

test('YON_FORMAT=global emits registry modules that prerender successfully', { timeout: 20000 }, async () => {
    const cwd = await createGlobalYonFixture()

    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        env: {
            ...process.env,
            YON_FORMAT: 'global',
        },
        stdout: 'pipe',
        stderr: 'pipe'
    })

    const [stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ])

    if (exitCode !== 0) throw new Error(stderr)
    expect(stdout).toContain('Bundle completed')
    expect(stderr).toBe('')

    const home = await readFile(path.join(cwd, 'dist', 'index.html'), 'utf8')
    const pageModule = await readFile(path.join(cwd, 'dist', 'pages', 'HTML.js'), 'utf8')
    const componentModule = await readFile(path.join(cwd, 'dist', 'components', 'badge.js'), 'utf8')

    expect(home).toContain('Badge: Global Yon')
    expect(pageModule).toContain('register("/pages/HTML.js"')
    expect(componentModule).toContain('register("/components/badge.js"')
    expect(pageModule).not.toContain('export default')
    expect(componentModule).not.toContain('export default')
})

test('async event handlers are awaited before Yon rerenders', { timeout: 20000 }, async () => {
    const cwd = await createAsyncEventFixture()

    const proc = Bun.spawn(['bun', bundleEntrypoint], {
        cwd,
        stdout: 'pipe',
        stderr: 'pipe'
    })

    const [_stdout, stderr, exitCode] = await Promise.all([
        decode(proc.stdout),
        decode(proc.stderr),
        proc.exited
    ])

    if (exitCode !== 0) throw new Error(stderr)
    expect(stderr).toBe('')

    const pageModulePath = path.join(cwd, 'dist', 'pages', 'HTML.js')
    const pageModule = await import(`${pathToFileURL(pageModulePath).href}?async-event=${Date.now()}`)
    const render = await pageModule.default()
    const initial = await render()
    const buttonId = initial.match(/<button[^>]* id="([^"]+)"/)?.[1]

    expect(buttonId).toBeDefined()
    expect(initial).toContain('>idle</p>')

    const updated = await render(buttonId)

    expect(updated).toContain('>phone-input</p>')
    expect(updated).not.toContain('>loading</p>')
})
