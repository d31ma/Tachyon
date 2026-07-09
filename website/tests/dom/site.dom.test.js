// @ts-check
import { afterAll, beforeAll, describe, expect, test } from 'bun:test'
import { Window } from 'happy-dom'
import { ensureBundle } from '../helpers/ensure-bundle.js'

const PROJECT_ROOT = import.meta.dir.replace(/[/\\]tests[/\\]dom$/, '')

/** @param {string} relativePath */
async function read(relativePath) {
    return Bun.file(`${PROJECT_ROOT}/${relativePath}`).text()
}

/** @param {string} html */
function parse(html) {
    const window = new Window()
    Object.assign(window, { SyntaxError })
    const document = window.document.implementation.createHTMLDocument()
    document.documentElement.innerHTML = html
    return { window, document }
}

/** @type {ReturnType<typeof parse>} */
let home
/** @type {ReturnType<typeof parse>} */
let atlas
/** @type {ReturnType<typeof parse>} */
let docs

beforeAll(async () => {
    await ensureBundle()
    home = parse(await read('dist/web/index.html'))
    atlas = parse(await read('dist/web/atlas/index.html'))
    docs = parse(await read('dist/web/docs/index.html'))
}, 120000)

afterAll(() => {
    home?.window.close()
    atlas?.window.close()
    docs?.window.close()
})

describe('homepage DOM', () => {
    test('ships the Tachyon bootstrap shell', async () => {
        const shell = await read('dist/web/index.html')
        expect(shell).toContain('spa-renderer.js')
        expect(shell).toContain('imports.js')
        expect(shell).toContain('fylo-browser-path')
    })

    test('renders the DuVay shell: app bar, mobile dropdown, footer', () => {
        expect(home.document.querySelector('w-app-bar')).toBeTruthy()
        // Below-desktop navigation is a right-aligned dropdown, matching FYLO's
        // mobile shell. No bottom bar: it overlapped the OS gesture area.
        expect(home.document.querySelector('button.header-burger[aria-controls="mobile-menu"]')).toBeTruthy()
        expect(home.document.querySelector('nav#mobile-menu[w-dropdown]')).toBeTruthy()
        expect(home.document.querySelector('w-navigation-drawer')).toBeFalsy()
        expect(home.document.querySelector('w-bottom-navigation')).toBeFalsy()
        expect(home.document.querySelector('w-footer')).toBeTruthy()
        expect(home.document.querySelector('img.brand-mark[src="/shared/assets/logo.svg"]')).toBeTruthy()
    })

    test('renders the hero and feature cards', () => {
        const hero = home.document.querySelector('.hero h1')
        expect(hero?.textContent).toContain('Ship the whole stack')
        const cards = home.document.querySelectorAll('.features-grid w-card')
        expect(cards.length).toBe(7)
        expect(home.document.querySelector('.hero-install code')?.textContent).toContain('ty init my-app')
        expect(home.document.querySelector('[data-tac-scope="home-yon"]')).toBeTruthy()
        expect(home.document.querySelector('.yon-languages')?.textContent).toContain('TypeScript')
        expect(home.document.querySelector('[data-tac-scope="home-targets"]')).toBeTruthy()
        expect(home.document.querySelector('.target-terminal pre code')?.textContent).toContain('ty bundle --target all')
    })

    test('links the primary destinations', () => {
        const hrefs = [...home.document.querySelectorAll('w-btn[href], a[href]')]
            .map((el) => el.getAttribute('href'))
        for (const target of ['/atlas', '/docs']) {
            expect(hrefs).toContain(target)
        }
    })

    test('mobile dropdown closes from href-bearing custom elements', async () => {
        const source = await read('client/components/site/header/tac.js')

        expect(source).toContain("closest('[href]')")
        expect(source).not.toContain("closest('a[href]')")
    })
})

describe('atlas DOM', () => {
    test('prerenders every capability panel', () => {
        for (const scope of [
            'panel-inputs', 'panel-native', 'panel-helpers', 'panel-live',
            'panel-realtime', 'panel-diagnostics', 'panel-polyglot', 'panel-desktop',
            'panel-inventory', 'panel-fylo', 'panel-users', 'panel-showcase',
            'panel-telemetry', 'stats-grid',
        ]) {
            expect(atlas.document.querySelector(`[data-tac-scope="${scope}"]`)).toBeTruthy()
        }
    })

    test('keeps semantic native elements in the studio panel', () => {
        expect(atlas.document.querySelector('#browser-studio canvas')).toBeTruthy()
        expect(atlas.document.querySelector('#browser-studio progress')).toBeTruthy()
        expect(atlas.document.querySelector('#browser-studio meter')).toBeTruthy()
        expect(atlas.document.querySelector('#browser-studio output')).toBeTruthy()
        expect(atlas.document.querySelector('#browser-studio time')).toBeTruthy()
        expect(atlas.document.querySelector('#browser-studio details')).toBeTruthy()
    })

    test('numbers the six atlas sections', () => {
        const ids = [...atlas.document.querySelectorAll('.atlas-section')].map((el) => el.id)
        expect(ids).toEqual(['compose', 'react', 'connect', 'store', 'observe', 'extend'])
    })
})

describe('docs DOM', () => {
    test('uses a DuVay-style documentation shell', () => {
        expect(docs.document.querySelector('.docs-shell')).toBeTruthy()
        expect(docs.document.querySelector('.docs-sidebar')).toBeTruthy()
        expect(docs.document.querySelector('.docs-main')).toBeTruthy()
        expect(docs.document.querySelector('.docs-page')).toBeTruthy()
        expect(docs.document.querySelector('site-footer')).toBeFalsy()
    })
})
