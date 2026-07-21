// @ts-check
import { describe, expect, test } from 'bun:test'
import { ensureBundle } from './helpers/ensure-bundle.js'

const PROJECT_ROOT = import.meta.dir.replace(/[/\\]tests$/, '')

/** @param {string} relativePath */
async function read(relativePath) {
    return Bun.file(`${PROJECT_ROOT}/${relativePath}`).text()
}

/** @param {string} relativePath */
async function exists(relativePath) {
    return Bun.file(`${PROJECT_ROOT}/${relativePath}`).exists()
}

describe('package setup', () => {
    test('uses the in-repo Tachyon workflows', async () => {
        const pkg = JSON.parse(await read('package.json'))

        expect(pkg.scripts.start).toBe('bun run serve')
        expect(pkg.scripts.serve).toBe('bun ../src/cli/serve.js')
        expect(pkg.scripts.bundle).toBe('bun ../src/cli/bundle.js')
        expect(pkg.scripts.test).toBe('bun test tests/site.test.js tests/dom/site.dom.test.js')
        expect(pkg.dependencies['@d31ma/tachyon']).toBe('link:..')
    })

    test('is frontend-only: no server, db or middleware directories', async () => {
        expect(await exists('server/routes/language/yon.js')).toBe(false)
        expect(await exists('middleware.js')).toBe(false)
        expect(await exists('db/schemas/items/manifest.json')).toBe(false)
    })
})

describe('bundle output', () => {
    test('prerenders the marketing, atlas and docs shells', async () => {
        await ensureBundle()

        expect(await exists('dist/web/index.html')).toBe(true)
        expect(await exists('dist/web/atlas/index.html')).toBe(true)
        expect(await exists('dist/web/docs/index.html')).toBe(true)
        expect(await exists('dist/web/spa-renderer.js')).toBe(true)
        expect(await exists('dist/web/imports.js')).toBe(true)
    }, 120000)

    test('inlines the vendored DuVay layer into imports.css', async () => {
        await ensureBundle()
        const css = await read('dist/web/imports.css')

        // DuVay core tokens prove the vendored stylesheet was inlined.
        expect(css).toContain('--w-bp-desktop')
        expect(css).toContain('.w-btn')
        // The site brand layer rides on top.
        expect(css).toContain('--tachyon-gradient')
    }, 120000)

    test('ships vendored DuVay assets and the logo', async () => {
        await ensureBundle()

        expect(await exists('dist/web/shared/assets/duvay/duvay-wc.min.js')).toBe(true)
        expect(await exists('dist/web/shared/assets/duvay/duvay.min.js')).toBe(true)
        expect(await exists('dist/web/shared/assets/duvay/LICENSE')).toBe(true)
        expect(await exists('dist/web/shared/assets/logo.svg')).toBe(true)
        expect(await exists('dist/web/shared/assets/wordmark.svg')).toBe(true)
        expect(await exists('dist/web/shared/data/docs.json')).toBe(true)
    }, 120000)

    test('is an installable PWA: manifest, icons and service worker', async () => {
        await ensureBundle()

        const shell = await read('dist/web/index.html')
        expect(shell).toContain('<link rel="manifest" href="/shared/assets/manifest.webmanifest">')
        expect(shell).toContain('<meta name="theme-color" content="#f8faf9">')
        expect(shell).toContain('<link rel="icon" type="image/svg+xml" href="/shared/assets/favicon.svg">')

        const manifest = JSON.parse(await read('dist/web/shared/assets/manifest.webmanifest'))
        expect(manifest.display).toBe('standalone')
        expect(manifest.start_url).toBe('/')
        expect(manifest.icons.length).toBeGreaterThanOrEqual(3)

        expect(await exists('dist/web/shared/assets/icon-192.png')).toBe(true)
        expect(await exists('dist/web/shared/assets/icon-512.png')).toBe(true)
        expect(await exists('dist/web/tachyon-sw.js')).toBe(true)
    }, 120000)

    test('compiles the polyglot companion panel without a client worker endpoint', async () => {
        await ensureBundle()

        expect(await exists('dist/web/components/panel/polyglot/tac.js')).toBe(true)
        expect(await exists('dist/web/components/panel/portablebridge/tac.js')).toBe(true)
        for (const language of ['javascript', 'dart', 'kotlin', 'swift', 'csharp'])
            expect(await exists(`dist/web/components/language/${language}/tac.js`)).toBe(true)
        expect(await exists('dist/web/workers')).toBe(false)
    }, 120000)
})

describe('frontend-only sources', () => {
    test('panels use companions and the in-browser fylo client, not server routes', async () => {
        const diagnostics = await read('client/components/panel/diagnostics/tac.ts')
        const inventory = await read('client/components/panel/inventory/tac.ts')
        const realtime = await read('client/components/panel/realtime/tac.js')
        const telemetry = await read('client/components/panel/telemetry/tac.ts')

        expect(diagnostics).toContain('companion ABI ready')
        expect(inventory).toContain("fylo['atlas-items']")
        expect(realtime).toContain('BroadcastChannel')
        expect(telemetry).toContain("fylo['atlas-spans']")
        for (const source of [diagnostics, inventory, realtime, telemetry]) {
            expect(source).not.toContain("fetch('/language")
            expect(source).not.toContain("fetch('/realtime")
        }
    })

    test('demonstrates the unprefixed Rust platform prelude', async () => {
        const portableBridge = await read('client/components/panel/portablebridge/tac.rs')

        expect(portableBridge).toContain('local_storage().set_item')
        expect(portableBridge).toContain('navigator().language()')
        expect(portableBridge).toContain('fylo().collection')
        expect(portableBridge).toContain('app().is_available()')
        expect(portableBridge).toContain('app().info()')
        expect(portableBridge).toContain('capabilities().state("geo.current")')
        expect(portableBridge).toContain('secrets.get')
        expect(portableBridge).not.toContain('self.tac.')
    })

    test('uses every supported companion language in the live component tree', async () => {
        const polyglot = await read('client/components/panel/polyglot/tac.html')
        const files = [
            'client/components/panel/polyglot/tac.ts',
            'client/components/panel/portablebridge/tac.rs',
            'client/components/language/javascript/tac.js',
            'client/components/language/dart/tac.dart',
            'client/components/language/kotlin/tac.kt',
            'client/components/language/swift/tac.swift',
            'client/components/language/csharp/tac.cs',
        ]

        for (const path of files)
            expect(await exists(path)).toBe(true)
        for (const tag of ['language-javascript', 'language-dart', 'language-kotlin', 'language-swift', 'language-csharp'])
            expect(polyglot).toContain(`<${tag}`)
    })
})

describe('docs content', () => {
    test('groups every docs topic for framework-style navigation', async () => {
        const docs = JSON.parse(await read('client/shared/data/docs.json'))
        const grouped = new Set(docs.groups.flatMap((group) => group.topics))

        expect(docs.groups.map((group) => group.title)).toEqual([
            'Start',
            'Tac frontend',
            'Yon backend',
            'Cookbook',
        ])
        expect([...grouped]).toEqual(docs.order)
        expect(Object.keys(docs.topics).sort()).toEqual([...grouped].sort())
    })

    test('documents polyglot Yon middleware with class-style and raw shim examples', async () => {
        const docs = JSON.parse(await read('client/shared/data/docs.json'))
        const yon = docs.topics.yon
        const text = JSON.stringify(yon)
        const code = yon.sections.map((section) => section.code ?? '').join('\n')

        expect(text).toContain('server/middleware/yon.<ext>')
        expect(text).toContain('class Middleware:')
        expect(text).toContain('Raw shim middleware')
        expect(code).toContain('"phase": "before"')
        expect(code).toContain('"action": "respond"')
    })

    test('documents frontend-only recipes that mimic backend and database features', async () => {
        const docs = JSON.parse(await read('client/shared/data/docs.json'))
        const cookbook = docs.topics.cookbook
        const text = JSON.stringify(cookbook)

        expect(cookbook.summary).toContain('frontend-only')
        expect(text).toContain('tac.kt')
        expect(text).toContain('fylo.users')
        expect(text).toContain('server/middleware/yon.<ext>')
        expect(text).toContain('environment')
    })

    test('documents device APIs and the target-accurate native capability boundary', async () => {
        const docs = JSON.parse(await read('client/shared/data/docs.json'))
        const platformApis = docs.topics['platform-apis']
        const text = JSON.stringify(platformApis)

        expect(text).toContain('devicePermissions')
        expect(text).toContain('auth.verifyUser')
        expect(text).toContain('media.getUserMedia')
        expect(text).toContain('host.on')
        expect(text).toContain('Desktop native capability matrix')
        expect(text).toContain('managedContentOrigins')
        expect(text).toContain('tachyon.host.json')
    })

    test('documents island hydration as a strict server-rendered component contract', async () => {
        const docs = JSON.parse(await read('client/shared/data/docs.json'))
        const templates = JSON.stringify(docs.topics.templates)

        for (const policy of ['load', 'idle', 'visible', 'interaction', 'never'])
            expect(templates).toContain(policy)
        expect(templates).toContain('JSON-serializable')
        expect(templates).toContain('combined with lazy')
    })

    test('documents native-by-default HTML rendering and strict fallback behavior', async () => {
        const docs = JSON.parse(await read('client/shared/data/docs.json'))
        const nativeRendering = JSON.stringify(docs.topics['native-rendering'])

        expect(nativeRendering).toContain('SwiftUI')
        expect(nativeRendering).toContain('Jetpack Compose')
        expect(nativeRendering).toContain('WinUI')
        expect(nativeRendering).toContain('GTK')
        expect(nativeRendering).toContain('tachyon.native-ui.json')
        expect(nativeRendering).toContain('tachyon.native-controller.js')
        expect(nativeRendering).toContain('automatically become isolated WebView boundaries')
        expect(nativeRendering).toContain('nativeUIAdapters')
    })
})
