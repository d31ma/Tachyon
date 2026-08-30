// @ts-check
import { afterAll, beforeAll, describe, expect, test } from 'bun:test'
import { Window } from 'happy-dom'
import { ensureBundle } from '../helpers/ensure-bundle.js'

const PROJECT_ROOT = import.meta.dir.replace(/[/\\]tests[/\\]dom$/, '')
const ATLAS_SECTIONS = ['overview', 'compose', 'react', 'connect', 'store', 'observe', 'extend']

/** @param {string} relativePath */
async function read(relativePath) {
    return Bun.file(`${PROJECT_ROOT}/${relativePath}`).text()
}

/** @param {string} html */
function parseHtml(html) {
    const window = new Window()
    Object.assign(window, { SyntaxError })
    const document = window.document.implementation.createHTMLDocument()
    document.documentElement.innerHTML = html
    return { window, document }
}

/** @param {unknown} value */
function planNodes(value) {
    /** @type {Record<string, any>[]} */
    const result = []
    /** @param {unknown} current */
    function visit(current) {
        if (Array.isArray(current)) {
            for (const item of current) visit(item)
            return
        }
        if (!current || typeof current !== 'object') return
        const node = /** @type {Record<string, any>} */ (current)
        if (typeof node.k === 'string') result.push(node)
        for (const key of ['nodes', 'children', 'template', 'slot']) visit(node[key])
    }
    visit(value)
    return result
}

/** @param {Record<string, any>} node @param {string} name */
function attribute(node, name) {
    return node.attributes?.find((/** @type {{ name: string }} */ item) => item.name === name)?.value ?? null
}

/** @param {Record<string, any>} node @param {string} token */
function hasClass(node, token) {
    return (attribute(node, 'class') ?? '').split(/\s+/).includes(token)
}

/** @param {Record<string, any>} node @returns {string} */
function nodeText(node) {
    const own = node.parts?.map((/** @type {{ value?: string }} */ part) => part.value ?? '').join('') ?? ''
    /** @type {Record<string, any>[]} */
    const nested = [...(node.children ?? []), ...(node.template ?? []), ...(node.slot ?? [])]
    return own + nested
        .map((/** @type {Record<string, any>} */ child) => child && typeof child === 'object' ? nodeText(child) : '')
        .join('')
}

/** @param {string} relativePath */
async function readPage(relativePath) {
    const shell = await read(relativePath)
    const parsed = parseHtml(shell)
    const script = parsed.document.querySelector('script#tachyon-view[type="application/json"]')
    if (!script?.textContent) throw new Error(`${relativePath} has no Tac render plan`)
    const plan = JSON.parse(script.textContent)
    return { ...parsed, shell, plan, nodes: planNodes(plan) }
}

/** @type {Awaited<ReturnType<typeof readPage>>} */
let home
/** @type {Awaited<ReturnType<typeof readPage>>} */
let atlas
/** @type {Record<string, Awaited<ReturnType<typeof readPage>>>} */
let atlasSections = {}
/** @type {Awaited<ReturnType<typeof readPage>>} */
let docs

beforeAll(async () => {
    await ensureBundle()
    home = await readPage('dist/web/index.html')
    atlas = await readPage('dist/web/atlas/index.html')
    for (const section of ATLAS_SECTIONS)
        atlasSections[section] = await readPage(`dist/web/atlas/${section}/index.html`)
    docs = await readPage('dist/web/docs/index.html')
}, 120000)

afterAll(() => {
    home?.window.close()
    atlas?.window.close()
    for (const section of Object.values(atlasSections)) section?.window.close()
    docs?.window.close()
})

describe('client-rendered bootstrap contract', () => {
    test('ships a render plan and the Tac client without an authored HTML subtree', async () => {
        const header = await read('dist/web/.tachyon/components/site-header.js')
        expect(home.shell).toContain('/.tachyon/tac-client.js')
        expect(home.shell).not.toContain('/.tachyon/islands.js')
        expect(home.plan.schemaVersion).toBe(1)
        expect(home.plan.nodes.length).toBeGreaterThan(0)
        expect(home.document.body.querySelector('noscript')).toBeTruthy()
        expect(home.document.body.querySelector('w-app-bar, main, w-footer')).toBeFalsy()
        expect(header).toContain("import '../../shared/scripts/imports.js'")
    })

    test('every public route uses the same client-owned rendering boundary', () => {
        for (const page of [home, atlas, ...Object.values(atlasSections), docs]) {
            expect(page.document.querySelector('script#tachyon-view[data-tachyon-runtime]')).toBeTruthy()
            expect(page.document.querySelector('script[src="/.tachyon/tac-client.js"]')).toBeTruthy()
            expect(page.document.body.querySelector('noscript')?.textContent).toContain('requires JavaScript')
            expect(page.document.body.querySelector('[data-tac-scope]')).toBeFalsy()
        }
    })
})

describe('homepage render plan', () => {
    test('contains the DuVay shell, mobile dropdown and footer', () => {
        expect(home.nodes.some((node) => node.tag === 'w-app-bar')).toBe(true)
        expect(home.nodes.some((node) => node.tag === 'button' && hasClass(node, 'header-burger') && attribute(node, 'aria-controls') === 'mobile-menu')).toBe(true)
        expect(home.nodes.some((node) => node.tag === 'nav' && attribute(node, 'id') === 'mobile-menu' && attribute(node, 'w-dropdown') !== null)).toBe(true)
        expect(home.nodes.some((node) => node.tag === 'w-navigation-drawer')).toBe(false)
        expect(home.nodes.some((node) => node.tag === 'w-bottom-navigation')).toBe(false)
        expect(home.nodes.some((node) => node.tag === 'w-footer')).toBe(true)
        expect(home.nodes.some((node) => node.tag === 'img' && hasClass(node, 'brand-mark') && attribute(node, 'src') === '/shared/assets/logo.svg')).toBe(true)
    })

    test('contains the hero, feature cards and native target guidance', () => {
        const serialized = JSON.stringify(home.plan)
        const features = home.nodes.find((node) => node.k === 'component' && node.name === 'home-features')
        const cards = planNodes(features?.template).filter((node) => node.tag === 'w-card' && attribute(node, 'title'))
        const titles = cards.map((node) => attribute(node, 'title'))
        expect(serialized).toContain('Ship the whole stack')
        expect(serialized).toContain('installCommand')
        expect(cards.length).toBe(8)
        expect(titles).toContain('Native HTML rendering')
        expect(titles).toContain('Client-owned rendering')
        expect(serialized).toContain('@Delegate')
        expect(serialized).toContain('@Relay')
        expect(serialized).toContain('mandatory architecture declarations')
        expect(serialized).toContain('YON_JAVASCRIPT_RUNTIME')
        expect(serialized).not.toContain('registered direct-protocol executables')
        expect(serialized).toContain('ty bundle --target all')
        expect(serialized).toContain('real native controls')
        expect(serialized).toContain('bounded fallback boundary')
        expect(serialized).toContain('no Tachyon/Yon app backend')
        expect(serialized).toContain('public fetch and SSE where shown')
        expect(serialized).not.toContain('never talks to a server')
        expect(serialized).not.toContain('--render-mode')
    })

    test('links the primary destinations', () => {
        const hrefs = home.nodes.map((node) => attribute(node, 'href')).filter(Boolean)
        expect(hrefs).toContain('/atlas')
        expect(hrefs).toContain('/docs')
    })

    test('keeps the mobile menu behavior and page scrollbar policy', async () => {
        const headerSource = await read('client/components/site/header/tac.js')
        const headerTemplate = parseHtml(await read('client/components/site/header/tac.html'))
        const trigger = headerTemplate.document.querySelector('button.header-burger[aria-controls="mobile-menu"]')
        const menu = headerTemplate.document.querySelector('nav#mobile-menu[w-dropdown]')
        const styles = await read('client/shared/styles/site.css')
        const quickstartStyles = await read('client/components/home/quickstart/tac.css')
        const headerStyles = await read('client/components/site/header/tac.css')
        const homeStyles = await read('client/pages/tac.css')

        expect(headerSource).toContain("closest('[href]')")
        expect(headerSource).not.toContain("closest('a[href]')")
        expect(trigger?.closest('w-container')).toBe(menu?.closest('w-container'))
        expect(headerTemplate.document.querySelector('a button, a w-chip')).toBeFalsy()
        expect(headerTemplate.document.querySelector('a.brand-link .brand-badge')?.textContent).toContain('Tac + Yon')
        expect(headerStyles).toContain('.brand-link')
        expect(styles).toContain('min-width: 44px')
        expect(quickstartStyles).toContain('grid-template-columns: minmax(0, 1fr)')
        expect(quickstartStyles).toContain('min-width: 0')
        expect(homeStyles).toContain('.atlas-teaser .w-card-subtitle')
        expect(homeStyles).toContain('white-space: normal')
        expect(homeStyles).toContain('text-overflow: clip')
        expect(styles).toContain('html,\nbody {\n  scrollbar-width: none;')
        expect(styles).toContain('html::-webkit-scrollbar')
        expect(styles).toContain('body::-webkit-scrollbar')
        headerTemplate.window.close()
    })
})

describe('atlas render plans', () => {
    test('use a docs-style shell with a section sidebar on every page', () => {
        for (const page of [atlas, ...Object.values(atlasSections)]) {
            expect(page.nodes.some((node) => hasClass(node, 'atlas-shell'))).toBe(true)
            expect(page.nodes.some((node) => hasClass(node, 'atlas-sidebar'))).toBe(true)
            expect(page.nodes.some((node) => hasClass(node, 'atlas-main'))).toBe(true)
            expect(page.nodes.some((node) => attribute(node, 'data-tac-scope') === 'atlas-sidebar')).toBe(true)
        }
    })

    test('the sidebar lists every section in order', () => {
        const hrefs = atlasSections.overview.nodes
            .filter((node) => node.tag === 'a' && (attribute(node, 'href') ?? '').startsWith('/atlas/'))
            .map((node) => attribute(node, 'href'))
        expect([...new Set(hrefs)]).toEqual(ATLAS_SECTIONS.map((section) => `/atlas/${section}`))
    })

    test('plans every capability component on its section page', () => {
        const componentsBySection = {
            overview: ['stats-grid'],
            compose: ['panel-inputs', 'panel-native'],
            react: ['panel-helpers', 'panel-live', 'panel-realtime'],
            connect: ['panel-diagnostics', 'panel-polyglot', 'panel-portablebridge', 'panel-desktop'],
            store: ['panel-showcase'],
            observe: ['panel-telemetry'],
        }
        for (const language of ['javascript', 'dart', 'kotlin', 'swift', 'csharp'])
            expect(atlasSections.connect.nodes.some((node) => node.k === 'component' && node.name === `language-${language}`)).toBe(true)
        for (const [section, scopes] of Object.entries(componentsBySection)) {
            for (const scope of scopes)
                expect(atlasSections[section].nodes.some((node) => attribute(node, 'data-tac-scope') === scope)).toBe(true)
        }
    })

    test('keeps semantic native elements in the studio panel', () => {
        const nodes = atlasSections.compose.nodes
        for (const tag of ['canvas', 'progress', 'meter', 'output', 'time', 'details'])
            expect(nodes.some((node) => node.tag === tag)).toBe(true)
    })

    test('each section page plans exactly its own numbered section', () => {
        for (const section of ['compose', 'react', 'connect', 'store', 'observe', 'extend']) {
            const ids = atlasSections[section].nodes
                .filter((node) => hasClass(node, 'atlas-section'))
                .map((node) => attribute(node, 'id'))
            expect(ids).toEqual([section])
            expect(atlasSections[section].nodes.filter((node) => node.tag === 'h1').length).toBe(1)
        }
        expect(atlasSections.overview.nodes.some((node) => hasClass(node, 'atlas-hero'))).toBe(true)
    })

    test('puts route content before the long sidebars on narrow screens', async () => {
        const atlasStyles = await read('client/pages/atlas/tac.css')
        const docsStyles = await read('client/pages/docs/tac.css')
        for (const styles of [atlasStyles, docsStyles]) {
            expect(styles).toContain('flex-direction: column')
            expect(styles).toContain('order: 1')
            expect(styles).toContain('order: 2')
        }
    })
})

describe('docs render plan', () => {
    test('uses a DuVay-style documentation shell', () => {
        expect(docs.nodes.some((node) => hasClass(node, 'docs-shell'))).toBe(true)
        expect(docs.nodes.some((node) => hasClass(node, 'docs-sidebar'))).toBe(true)
        expect(docs.nodes.some((node) => hasClass(node, 'docs-main'))).toBe(true)
        expect(docs.nodes.some((node) => hasClass(node, 'docs-page'))).toBe(true)
        expect(docs.nodes.some((node) => node.k === 'component' && node.name === 'site-footer')).toBe(false)
        expect(nodeText(docs.plan)).not.toContain('undefined')
    })
})
