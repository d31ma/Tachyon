import { afterAll, beforeAll, describe, expect, test } from 'bun:test'
import { GlobalRegistrator } from '@happy-dom/global-registrator'
import {
    cleanBooleanAttrs,
    findEventTarget,
    morphChildren,
    parseFragment,
    parseParams,
    resolveHandler,
} from '../../src/runtime/dom-helpers.js'

beforeAll(() => {
    GlobalRegistrator.register()
})

afterAll(() => {
    GlobalRegistrator.unregister()
})

describe('findEventTarget', () => {
    test('finds the closest declarative event target', () => {
        document.body.innerHTML = `
          <div @click="outer()">
            <button id="child"><span id="inner">Tap</span></button>
          </div>
        `

        const target = findEventTarget(document.getElementById('inner'), 'click')
        expect(target?.getAttribute('@click')).toBe('outer()')
    })
})

describe('cleanBooleanAttrs', () => {
    test('removes false boolean-like attributes and keeps true values', () => {
        document.body.innerHTML = `
          <button id="a" disabled="false"></button>
          <button id="b" selected="false"></button>
          <button id="c" checked="true"></button>
        `

        cleanBooleanAttrs()

        expect(document.getElementById('a')?.hasAttribute('disabled')).toBe(false)
        expect(document.getElementById('b')?.hasAttribute('selected')).toBe(false)
        expect(document.getElementById('c')?.getAttribute('checked')).toBe('true')
    })
})

describe('morphChildren', () => {
    test('updates keyed nodes in place and syncs attributes', () => {
        document.body.innerHTML = `<div id="root"><button id="save" class="old">Old</button></div>`
        const root = document.getElementById('root')!
        const original = document.getElementById('save')

        morphChildren(root, parseFragment(`<button id="save" class="new">New</button>`))

        const updated = document.getElementById('save')
        expect(updated).toBe(original)
        expect(updated?.className).toBe('new')
        expect(updated?.textContent).toBe('New')
    })

    test('preserves lazy content when requested', () => {
        document.body.innerHTML = `<div id="root"><div id="lazy-shell"><strong>Loaded content</strong></div></div>`
        const root = document.getElementById('root')!

        morphChildren(root, parseFragment(`<div id="lazy-shell"></div>`), {
            preserveElement: (el) => el.id === 'lazy-shell'
        })

        expect(document.querySelector('#lazy-shell strong')?.textContent).toBe('Loaded content')
    })
})

describe('routing helpers', () => {
    test('resolves the best matching route and fills slugs', () => {
        const routes = new Map<string, Record<string, number>>([
            ['/', {}],
            ['/docs', {}],
            ['/api/:version', { ':version': 1 }],
            ['/api/:version/users', { ':version': 1 }],
        ])
        const slugs: Record<string, string> = {}

        const match = resolveHandler('/api/v2/users', routes, slugs)

        expect(match).toBe('/api/:version/users')
        expect(slugs.version).toBe('v2')
    })

    test('parses path params into typed values', () => {
        expect(parseParams(['42', 'true', 'null', 'hello'])).toEqual([42, true, null, 'hello'])
    })
})
