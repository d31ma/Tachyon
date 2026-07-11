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
/** @type {Record<string, ReturnType<typeof parse>>} */
let atlasSections = {}
/** @type {ReturnType<typeof parse>} */
let docs

const ATLAS_SECTIONS = ['overview', 'compose', 'react', 'connect', 'store', 'observe', 'extend']

beforeAll(async () => {
    await ensureBundle()
    home = parse(await read('dist/web/index.html'))
    atlas = parse(await read('dist/web/atlas/index.html'))
    for (const section of ATLAS_SECTIONS)
        atlasSections[section] = parse(await read(`dist/web/atlas/${section}/index.html`))
    docs = parse(await read('dist/web/docs/index.html'))
}, 120000)

afterAll(() => {
    home?.window.close()
    atlas?.window.close()
    for (const section of Object.values(atlasSections)) section?.window.close()
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
        expect(cards.length).toBe(8)
        expect(home.document.querySelector('.hero-install code')?.textContent).toContain('ty init my-app')
        expect(home.document.querySelector('[data-tac-scope="home-yon"]')).toBeTruthy()
        expect(home.document.querySelector('.yon-languages')?.textContent).toContain('TypeScript')
        expect(home.document.querySelector('[data-tac-scope="home-targets"]')).toBeTruthy()
        expect(home.document.querySelector('.target-terminal pre code')?.textContent).toContain('ty bundle --target all')
        expect([...cards].map((card) => card.getAttribute('title'))).toContain('Permissioned device APIs')
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
    test('uses a docs-style shell with a section sidebar on every page', () => {
        for (const page of [atlas, ...Object.values(atlasSections)]) {
            expect(page.document.querySelector('.atlas-shell')).toBeTruthy()
            expect(page.document.querySelector('.atlas-sidebar')).toBeTruthy()
            expect(page.document.querySelector('.atlas-main')).toBeTruthy()
            expect(page.document.querySelector('[data-tac-scope="atlas-sidebar"]')).toBeTruthy()
        }
    })

    test('the sidebar lists every section in order', () => {
        const hrefs = [...atlasSections.overview.document.querySelectorAll('.atlas-sidebar-list a')]
            .map((el) => el.getAttribute('href'))
        expect(hrefs).toEqual(ATLAS_SECTIONS.map((section) => `/atlas/${section}`))
    })

    test('prerenders every capability panel on its section page', () => {
        const panelsBySection = {
            overview: ['stats-grid'],
            compose: ['panel-inputs', 'panel-native'],
            react: ['panel-helpers', 'panel-live', 'panel-realtime'],
            connect: [
                'panel-diagnostics', 'panel-polyglot', 'panel-portablebridge', 'panel-desktop',
                'language-javascript', 'language-dart', 'language-kotlin', 'language-swift', 'language-csharp',
            ],
            store: ['panel-inventory', 'panel-fylo', 'panel-users', 'panel-showcase'],
            observe: ['panel-telemetry'],
        }
        for (const [section, scopes] of Object.entries(panelsBySection)) {
            for (const scope of scopes) {
                expect(atlasSections[section].document.querySelector(`[data-tac-scope="${scope}"]`)).toBeTruthy()
            }
        }
    })

    test('keeps semantic native elements in the studio panel', () => {
        const compose = atlasSections.compose
        expect(compose.document.querySelector('#browser-studio canvas')).toBeTruthy()
        expect(compose.document.querySelector('#browser-studio progress')).toBeTruthy()
        expect(compose.document.querySelector('#browser-studio meter')).toBeTruthy()
        expect(compose.document.querySelector('#browser-studio output')).toBeTruthy()
        expect(compose.document.querySelector('#browser-studio time')).toBeTruthy()
        expect(compose.document.querySelector('#browser-studio details')).toBeTruthy()
    })

    test('each section page renders exactly its own numbered section', () => {
        for (const section of ['compose', 'react', 'connect', 'store', 'observe', 'extend']) {
            const ids = [...atlasSections[section].document.querySelectorAll('.atlas-section')].map((el) => el.id)
            expect(ids).toEqual([section])
        }
        expect(atlasSections.overview.document.querySelector('.atlas-hero')).toBeTruthy()
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
