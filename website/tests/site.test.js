// @ts-check
import { describe, expect, test } from 'bun:test'
import path from 'node:path'
import { ensureBundle } from './helpers/ensure-bundle.js'

const PROJECT_ROOT = import.meta.dir.replace(/[/\\]tests$/, '')
const REPOSITORY_ROOT = path.resolve(PROJECT_ROOT, '..')

/** @param {string} relativePath */
async function read(relativePath) {
    return Bun.file(`${PROJECT_ROOT}/${relativePath}`).text()
}

/** @param {string} relativePath */
async function exists(relativePath) {
    return Bun.file(`${PROJECT_ROOT}/${relativePath}`).exists()
}

describe('Rust website workflow', () => {
    test('builds and serves through the Rust workspace', async () => {
        const pkg = JSON.parse(await read('package.json'))
        const manifest = JSON.parse(await read('tachyon.json'))
        expect(pkg.scripts.bundle).toBe('ty bundle')
        expect(pkg.scripts.serve).toBe('ty serve')
        expect(pkg.scripts.preview).toBe('ty preview')
        expect(pkg.dependencies['@d31ma/tachyon']).toBeUndefined()
        expect(manifest.application.name).toBe('Tachyon')
        expect(manifest.application.version).toBe(pkg.version)
    })

    test('publishes routes, runtime assets and client/shared', async () => {
        await ensureBundle()
        for (const path of [
            'dist/web/index.html',
            'dist/web/atlas/overview/index.html',
            'dist/web/atlas/connect/index.html',
            'dist/web/docs/index.html',
            'dist/web/.tachyon/tac-client.js',
            'dist/web/shared/assets/logo.svg',
            'dist/web/shared/assets/duvay/duvay-wc.min.js',
            'dist/web/shared/data/docs.json',
            'dist/web/shared/scripts/imports.js',
            'dist/web/tachyon-sw.js',
        ]) expect(await exists(path)).toBe(true)
    }, 180000)

    test('compiles every real-language browser companion', async () => {
        await ensureBundle()
        for (const path of [
            'dist/web/.tachyon/components/language-javascript.js',
            'dist/web/.tachyon/components/language-dart.mjs',
            'dist/web/.tachyon/components/language-kotlin.mjs',
            'dist/web/.tachyon/components/language-swift.wasm',
            'dist/web/.tachyon/components/language-csharp.mjs',
            'dist/web/.tachyon/components/panel-portablebridge.wasm',
        ]) expect(await exists(path)).toBe(true)
    }, 180000)

    test('keeps authored companions beside ADR 0011 real-compiler sidecars', async () => {
        const csharp = await read('client/components/language/csharp/tachyon-wasm.cs')
        const dart = await read('client/components/language/dart/tachyon-wasm.dart')
        const kotlin = await read('client/components/language/kotlin/tachyon-wasm.kt')
        const swift = await read('client/components/language/swift/tachyon-wasm.swift')
        const rust = await read('client/components/panel/portablebridge/tachyon-wasm.rs')
        const quickstart = await read('client/components/home/quickstart/tachyon-island.js')
        expect(csharp).toContain('Dictionary<string, TacMember> Tac')
        expect(dart).toContain('final tac =')
        expect(kotlin).toContain('val tac =')
        expect(swift).toContain('let tac: [String: TacMember]')
        expect(rust).toContain('pub extern "C" fn tac_invoke')
        expect(quickstart).toContain('hydrate(root)')
        expect(await read('client/components/home/quickstart/tac.js')).toContain('tac.onMount')
        expect(await read('client/components/language/csharp/tac.cs')).toContain(': Tac')
        expect(await read('client/components/language/dart/tac.dart')).toContain('extends Tac')
        expect(await read('client/components/language/kotlin/tac.kt')).toContain(': Tac()')
        expect(await read('client/components/language/swift/tac.swift')).toContain(': Tac')
    })
})

describe('cutover documentation', () => {
    test('records real toolchains and intentional server boundaries', async () => {
        const docs = JSON.parse(await read('client/shared/data/docs.json'))
        const polyglot = JSON.stringify(docs.topics.polyglot)
        const yon = JSON.stringify(docs.topics.yon)
        const native = JSON.stringify(docs.topics['native-rendering'])
        const platformApis = JSON.stringify(docs.topics['platform-apis'])
        expect(polyglot).toContain('compiled by rustc, dart, kotlinc-js, swiftc and dotnet')
        expect(polyglot).toContain('Declare the ABI')
        expect(polyglot).toContain('does not discover or enforce legacy OPTIONS.schema.json')
        expect(yon).toContain('does not generate OpenAPI')
        expect(yon).toContain('does not emit OpenTelemetry spans')
        expect(yon).toContain('/.tachyon/topics/releases')
        expect(yon).toContain('request.parameters')
        expect(yon).toContain('middleware.rb')
        expect(yon).toContain('x-tachyon-request-id')
        expect(native).toContain('Android platform Views')
        expect(native).toContain('Win32 common controls')
        expect(native).toContain('do not embed QuickJS or JavaScriptCore')
        expect(platformApis).toContain('contracts only')
        expect(platformApis).toContain('do not currently implement public contentSurface')
        expect(platformApis).not.toContain('screenCapture.captureWindow')
    })

    test('catalogs every Tachyon-owned environment variable with scope and use case', async () => {
        const docs = JSON.parse(await read('client/shared/data/docs.json'))
        const topic = docs.topics.environment
        const entries = topic.sections.flatMap((/** @type {{ variables?: Array<{ name: string, values: string, use: string, scope: string }> }} */ section) => section.variables ?? [])
        const documented = entries.map((/** @type {{ name: string }} */ entry) => entry.name)

        expect(docs.order).toContain('environment')
        expect(documented.length).toBe(new Set(documented).size)
        for (const entry of entries) {
            expect(entry.name).toMatch(/^(TAC|YON)_[A-Z0-9_]+$/)
            expect(entry.values.trim()).not.toBe('')
            expect(entry.use.trim()).not.toBe('')
            expect(entry.scope.trim()).not.toBe('')
        }

        const discovered = new Set()
        const token = /\b(?:TAC|YON)_[A-Z0-9_]+\b/g
        const extensions = new Set(['.rs', '.js', '.mjs', '.cjs', '.ts', '.sh', '.ps1', '.yml', '.yaml'])
        const roots = ['crates', 'scripts', '.github', 'website/tests']
        for (const directory of roots) {
            const glob = new Bun.Glob('**/*')
            for await (const relative of glob.scan({ cwd: path.join(REPOSITORY_ROOT, directory), onlyFiles: true })) {
                if (!extensions.has(path.extname(relative))) continue
                if (relative.split(path.sep).includes('fixtures')) continue
                const source = await Bun.file(path.join(REPOSITORY_ROOT, directory, relative)).text()
                for (const match of source.matchAll(token)) discovered.add(match[0])
            }
        }
        for (const file of ['install.sh', 'install.ps1']) {
            const source = await Bun.file(path.join(REPOSITORY_ROOT, file)).text()
            for (const match of source.matchAll(token)) discovered.add(match[0])
        }
        for (const constant of [
            'TAC_CONTROL_ELEMENT_SET',
            'TAC_CLIENT_RUNTIME',
            'TAC_MODULE_FILE',
            'TAC_NATIVE_BRIDGE_ABI_VERSION',
            'TAC_ROOT',
            'YON_ROOT',
        ]) discovered.delete(constant)

        expect(documented.sort()).toEqual([...discovered].sort())
    })
})
