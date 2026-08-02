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
            'dist/web/.tachyon/islands.js',
            'dist/web/.tachyon/events.js',
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

    test('keeps legacy companions beside ADR 0011 real-compiler sidecars', async () => {
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
        expect(polyglot).toContain('compiled by rustc, dart, kotlinc-js, swiftc and dotnet')
        expect(polyglot).toContain('Declare the ABI')
        expect(yon).toContain('does not generate OpenAPI')
        expect(yon).toContain('does not emit OpenTelemetry spans')
        expect(yon).toContain('/.tachyon/topics/releases')
    })
})
