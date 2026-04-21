import { afterAll, beforeAll, describe, expect, test } from 'bun:test'
import { Window } from 'happy-dom'
import path from 'node:path'
import os from 'node:os'

const TEMPLATE_PATH = new URL('../../src/compiler/render-template.js', import.meta.url).pathname

/**
 * Builds a factory from render-template.js with test code injected into the
 * script slot. Results are returned via globalThis.__ty_test__ to bridge
 * the ESM module boundary.
 */
async function buildTestFactory(testScript: string, testInners?: string): Promise<(props?: unknown) => Promise<unknown>> {
    const source = await Bun.file(TEMPLATE_PATH).text()
    const modified = source
        .replace('// imports', '')
        .replace('// script', testScript)
        .replace('// inners', testInners ?? '')
    const tmpPath = path.join(os.tmpdir(), `tachyon-tpl-${Bun.randomUUIDv7()}.js`)
    await Bun.write(tmpPath, modified)
    const { default: factory } = await import(tmpPath)
    return factory as (props?: unknown) => Promise<unknown>
}

function testResults(): Record<string, unknown> {
    const results: Record<string, unknown> = {}
    ;(globalThis as Record<string, unknown>).__ty_test__ = results
    return results
}

// ── Server-side (no window) ────────────────────────────────────────────────────

describe('render-template server-side (no window)', () => {
    test('isBrowser is false and isServer is true', async () => {
        const r = testResults()
        const factory = await buildTestFactory(`__ty_test__.isBrowser = isBrowser; __ty_test__.isServer = isServer`)
        await factory()
        expect(r.isBrowser).toBe(false)
        expect(r.isServer).toBe(true)
    })

    test('onMount is a no-op — callback is never registered or called', async () => {
        const r = testResults()
        r.called = false
        const factory = await buildTestFactory(`onMount(() => { __ty_test__.called = true })`)
        await factory()
        expect(r.called).toBe(false)
    })

    test('inject returns fallback when window is absent', async () => {
        const r = testResults()
        const factory = await buildTestFactory(`__ty_test__.value = inject('key', 'fallback')`)
        await factory()
        expect(r.value).toBe('fallback')
    })

    test('inject returns undefined fallback by default', async () => {
        const r = testResults()
        const factory = await buildTestFactory(`__ty_test__.value = inject('key')`)
        await factory()
        expect(r.value).toBeUndefined()
    })

    test('persist returns [initialValue, noop] without window', async () => {
        const r = testResults()
        const factory = await buildTestFactory(`
            const [val, save] = persist('k', 42)
            __ty_test__.val = val
            __ty_test__.noopResult = save('anything')
        `)
        await factory()
        expect(r.val).toBe(42)
        expect(r.noopResult).toBeUndefined()
    })

    test('rerender is a no-op without window', async () => {
        const factory = await buildTestFactory(`rerender()`)
        expect(async () => await factory()).not.toThrow()
    })
})

// ── Browser-side (with happy-dom window) ──────────────────────────────────────

describe('render-template browser-side (with window)', () => {
    let windowInstance: Window
    let previousGlobals: Record<string, unknown>

    beforeAll(() => {
        windowInstance = new Window()
        previousGlobals = {
            window: (globalThis as Record<string, unknown>).window,
            sessionStorage: (globalThis as Record<string, unknown>).sessionStorage,
            CustomEvent: (globalThis as Record<string, unknown>).CustomEvent,
        }
        Object.assign(globalThis, {
            window: windowInstance,
            sessionStorage: windowInstance.sessionStorage,
            CustomEvent: windowInstance.CustomEvent,
        })
    })

    afterAll(async () => {
        await windowInstance.happyDOM.close()
        Object.assign(globalThis, previousGlobals)
    })

    test('isBrowser is true and isServer is false', async () => {
        const r = testResults()
        const factory = await buildTestFactory(`__ty_test__.isBrowser = isBrowser; __ty_test__.isServer = isServer`)
        await factory()
        expect(r.isBrowser).toBe(true)
        expect(r.isServer).toBe(false)
    })

    test('onMount pushes callback to window.__ty_onMount_queue__', async () => {
        delete (windowInstance as Record<string, unknown>).__ty_onMount_queue__
        const r = testResults()
        r.called = false
        const factory = await buildTestFactory(`onMount(() => { __ty_test__.called = true })`)
        await factory()
        const queue = (windowInstance as Record<string, unknown>).__ty_onMount_queue__ as Array<() => void | Promise<void>>
        expect(Array.isArray(queue)).toBe(true)
        expect(queue.length).toBe(1)
        queue[0]()
        expect(r.called).toBe(true)
    })

    test('multiple onMount calls append to the queue in order', async () => {
        delete (windowInstance as Record<string, unknown>).__ty_onMount_queue__
        const r = testResults()
        r.order = []
        const factory = await buildTestFactory(`
            onMount(() => { __ty_test__.order.push(1) })
            onMount(() => { __ty_test__.order.push(2) })
        `)
        await factory()
        const queue = (windowInstance as Record<string, unknown>).__ty_onMount_queue__ as Array<() => void | Promise<void>>
        queue.forEach(fn => fn())
        expect(r.order).toEqual([1, 2])
    })

    test('inject retrieves value from window.__ty_context__', async () => {
        const ctx = new Map([['apiBase', 'https://api.example.com']])
        ;(windowInstance as Record<string, unknown>).__ty_context__ = ctx
        const r = testResults()
        const factory = await buildTestFactory(`__ty_test__.value = inject('apiBase')`)
        await factory()
        expect(r.value).toBe('https://api.example.com')
    })

    test('inject returns fallback for absent key', async () => {
        ;(windowInstance as Record<string, unknown>).__ty_context__ = new Map()
        const r = testResults()
        const factory = await buildTestFactory(`__ty_test__.value = inject('missing', 'default')`)
        await factory()
        expect(r.value).toBe('default')
    })

    test('provide sets value in window.__ty_context__', async () => {
        const ctx = new Map()
        ;(windowInstance as Record<string, unknown>).__ty_context__ = ctx
        const factory = await buildTestFactory(`provide('svc', { url: '/api' })`)
        await factory()
        expect(ctx.get('svc')).toEqual({ url: '/api' })
    })

    test('persist restores value from sessionStorage on factory init', async () => {
        windowInstance.sessionStorage.setItem('testRestore', JSON.stringify({ id: 7 }))
        const r = testResults()
        const factory = await buildTestFactory(`const [val] = persist('testRestore', null); __ty_test__.val = val`)
        await factory()
        expect(r.val).toEqual({ id: 7 })
        windowInstance.sessionStorage.removeItem('testRestore')
    })

    test('persist returns initialValue when key is absent from sessionStorage', async () => {
        windowInstance.sessionStorage.removeItem('absentKey')
        const r = testResults()
        const factory = await buildTestFactory(`const [val] = persist('absentKey', 'init'); __ty_test__.val = val`)
        await factory()
        expect(r.val).toBe('init')
    })

    test('persist save writes JSON to sessionStorage', async () => {
        windowInstance.sessionStorage.removeItem('writeKey')
        const factory = await buildTestFactory(`const [, save] = persist('writeKey', null); save({ persisted: true })`)
        await factory()
        const stored = windowInstance.sessionStorage.getItem('writeKey')
        expect(JSON.parse(stored ?? 'null')).toEqual({ persisted: true })
    })

    test('rerender calls window.__ty_rerender', async () => {
        const r = testResults()
        r.called = false
        ;(windowInstance as Record<string, unknown>).__ty_rerender = () => { r.called = true }
        const factory = await buildTestFactory(`rerender()`)
        await factory()
        expect(r.called).toBe(true)
        delete (windowInstance as Record<string, unknown>).__ty_rerender
    })

    test('rerender is safe when window.__ty_rerender is absent', async () => {
        delete (windowInstance as Record<string, unknown>).__ty_rerender
        const factory = await buildTestFactory(`rerender()`)
        expect(async () => await factory()).not.toThrow()
    })

    test('persist is safe with malformed sessionStorage value', async () => {
        windowInstance.sessionStorage.setItem('badJson', 'not-valid-json{{{')
        const r = testResults()
        const factory = await buildTestFactory(`const [val] = persist('badJson', 'safe'); __ty_test__.val = val`)
        await factory()
        expect(r.val).toBe('safe')
        windowInstance.sessionStorage.removeItem('badJson')
    })
})

// ── Prerender-environment (window = globalThis, no sessionStorage) ─────────────

describe('render-template prerender-environment (window=globalThis, __ty_prerender__=true)', () => {
    let previousGlobals: Record<string, unknown>

    beforeAll(() => {
        previousGlobals = {
            window: (globalThis as Record<string, unknown>).window,
            __ty_prerender__: (globalThis as Record<string, unknown>).__ty_prerender__,
        }
        Object.assign(globalThis, {
            window: globalThis,
            __ty_prerender__: true,
        })
    })

    afterAll(() => {
        Object.assign(globalThis, previousGlobals)
        if (previousGlobals.__ty_prerender__ === undefined) {
            Reflect.deleteProperty(globalThis as Record<string, unknown>, '__ty_prerender__')
        }
    })

    test('isBrowser is false and isServer is true despite window being set', async () => {
        const r = testResults()
        const factory = await buildTestFactory(`__ty_test__.isBrowser = isBrowser; __ty_test__.isServer = isServer`)
        await factory()
        expect(r.isBrowser).toBe(false)
        expect(r.isServer).toBe(true)
    })

    test('onMount does not push to any queue during prerender', async () => {
        const r = testResults()
        r.called = false
        const factory = await buildTestFactory(`onMount(() => { __ty_test__.called = true })`)
        await factory()
        expect(r.called).toBe(false)
        expect((globalThis as Record<string, unknown>).__ty_onMount_queue__).toBeUndefined()
    })

    test('persist returns [initialValue, noop] during prerender without crashing', async () => {
        const r = testResults()
        const factory = await buildTestFactory(`const [val, save] = persist('someKey', 99); __ty_test__.val = val; save(100)`)
        await factory()
        expect(r.val).toBe(99)
    })

    test('inject returns fallback during prerender', async () => {
        const r = testResults()
        const factory = await buildTestFactory(`__ty_test__.value = inject('k', 'prerender-fallback')`)
        await factory()
        expect(r.value).toBe('prerender-fallback')
    })
})

// ── Async event handler re-rendering ───────────────────────────────────────────

describe('ty_invokeEvent awaits async handlers', () => {
    test('state change after await inside handler is visible when render completes', async () => {
        const r = testResults()

        const script = `
            let step = 'start'
            async function next() {
                await new Promise(resolve => setTimeout(resolve, 50))
                step = 'done'
            }
        `
        const inners = `
            await ty_invokeEvent('testhash', async ($event) => { const __event__ = $event; return next() })
            ;(globalThis).__ty_test__.step = step
        `

        const factory = await buildTestFactory(script, inners)
        const render = await factory()
        // Trigger the event by passing the matching element ID
        await render('ty-testhash-0', null)
        expect(r.step).toBe('done')
    })

    test('sync handler still works with async callback and return', async () => {
        const r = testResults()

        const script = `
            let count = 0
        `
        const inners = `
            await ty_invokeEvent('synchash', async ($event) => { const __event__ = $event; return count++ })
            ;(globalThis).__ty_test__.count = count
        `

        const factory = await buildTestFactory(script, inners)
        const render = await factory()
        await render('ty-synchash-0', null)
        expect(r.count).toBe(1)
    })

    test('handler returning a promise chain is awaited', async () => {
        const r = testResults()

        const script = `
            let value = 'pending'
            function fetchData() {
                return new Promise(resolve => setTimeout(resolve, 30))
                    .then(() => { value = 'resolved' })
            }
        `
        const inners = `
            await ty_invokeEvent('chainhash', async ($event) => { const __event__ = $event; return fetchData() })
            ;(globalThis).__ty_test__.value = value
        `

        const factory = await buildTestFactory(script, inners)
        const render = await factory()
        await render('ty-chainhash-0', null)
        expect(r.value).toBe('resolved')
    })

    test('non-matching element ID does not execute handler', async () => {
        const r = testResults()
        r.called = false

        const script = `
            async function handler() {
                ;(globalThis).__ty_test__.called = true
            }
        `
        const inners = `
            await ty_invokeEvent('skiphash', async ($event) => { const __event__ = $event; return handler() })
        `

        const factory = await buildTestFactory(script, inners)
        const render = await factory()
        // Pass a non-matching ID
        await render('ty-wrongid-0', null)
        expect(r.called).toBe(false)
    })
})
