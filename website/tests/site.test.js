// @ts-nocheck -- validated by the Tachyon compiler and website contract gates.
// Build-output contract for the website.
//
// The site is the framework's own showcase, so what it publishes is evidence:
// if a route stops linking its stylesheet or the language snippets stop
// shipping, the page is broken in a way a screenshot review would miss.

import { beforeAll, describe, expect, test } from 'bun:test'
import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { ensureBundle } from './helpers/ensure-bundle.js'

const root = fileURLToPath(new URL('..', import.meta.url))
const read = (relative) => readFile(new URL(relative, new URL('..', import.meta.url)), 'utf8')

beforeAll(ensureBundle)

describe('published output', () => {
  test('every route links its stylesheet and the client runtime', async () => {
    for (const [route, stylesheet, module] of [
      ['dist/web/index.html', '/style.css', '/client.js'],
      ['dist/web/docs/index.html', '/docs/style.css', '/docs/client.js'],
      ['dist/web/docs/_topic/index.html', '/docs/_topic/style.css', '/docs/_topic/client.js'],
    ]) {
      const html = await read(route)
      // The current compiler emits route-owned assets; each thin route entry
      // imports the shared design system and browser bootstrap.
      expect(html).toContain(stylesheet)
      expect(html).toContain(`\"module\":\"${module}\"`)
      expect(html).toContain('/.tachyon/tac-client.js')
    }
  })

  test('the shared stylesheet orders DuVay below Tailwind', async () => {
    const css = await read('client/shared/styles/site.css')
    // Component classes and utilities share specificity, so the cascade is
    // what makes "DuVay for rigidity, Tailwind for fluidity" true.
    expect(css).toContain('@layer duvay, theme, base, components, brand, utilities;')
    expect(css).toContain('layer(duvay)')
  })

  test('Tailwind is imported without Preflight', async () => {
    const source = await read('client/shared/styles/tailwind.src.css')
    // Preflight would strip the heading sizes and form controls DuVay styles.
    expect(source).toContain('tailwindcss/utilities.css')
    expect(source).not.toContain('"tailwindcss"')
  })
})

describe('language coverage', () => {
  test('every language Tachyon supports has a snippet in both layers', async () => {
    const languages = JSON.parse(await read('client/shared/data/languages.json'))
    // Tac is one route across every target — a view, a stylesheet and one
    // companion per language — so its languages are named by file extension
    // rather than by an id.
    const tac = languages.tac.files.map((file) => file.name.split('.').pop())
    const yon = languages.yon.entries.map((entry) => entry.id)

    // A component companion is JS or TS; a page companion may be any of these,
    // each compiled for the targets its toolchain builds.
    for (const extension of ['js', 'ts', 'rs', 'swift', 'kt', 'cs']) {
      expect(tac).toContain(extension)
    }
    // And the two halves that are the same on every target.
    for (const extension of ['html', 'css']) {
      expect(tac).toContain(extension)
    }
    // The eight languages whose own syntax can carry an annotation, because a
    // Yon example is a layered suite now and the layer is declared on the
    // class. Go and Ruby are absent: neither has annotation syntax at all,
    // where JavaScript only needed a runtime that transpiles decorators and
    // Rust only needed its attributes' proc-macro crate built for it.
    for (const id of ['javascript', 'typescript', 'python', 'java', 'csharp',
                      'kotlin', 'php', 'rust']) {
      expect(yon).toContain(id)
    }
    expect(yon).toHaveLength(8)
    for (const file of languages.tac.files) {
      expect(file.code.length).toBeGreaterThan(20)
      expect(file.name).toMatch(/\/tac\.[a-z]+$/)
    }
    // Each Yon language is a layered suite: a controller, and the layers it
    // declares beneath itself.
    for (const entry of languages.yon.entries) {
      const names = entry.files.map((file) => file.name)
      expect(names.some((name) => name.startsWith('server/routes/'))).toBe(true)
      expect(names.some((name) => name.startsWith('server/services/'))).toBe(true)
      expect(names.some((name) => name.startsWith('server/repositories/'))).toBe(true)
    }
  })

  test('every companion language shows the same class, spelled its own way', async () => {
    // A developer writes one companion and then writes it again for another
    // target. If the samples disagree about what a companion looks like, the
    // second one is a guess. So all six declare the same three members.
    const languages = JSON.parse(await read('client/shared/data/languages.json'))
    const companions = languages.tac.files.filter((file) => /\.(js|ts|rs|swift|kt|cs)$/.test(file.name))
    expect(companions.length).toBe(6)
    for (const entry of companions) {
      const code = entry.code
      // A class body — or, in Rust, the struct and impl that stand in for one.
      expect(code).toMatch(/\b(class|struct)\b/)
      for (const member of ['count', 'runtime', 'doubled']) {
        // C# writes them PascalCase, which the generator lowercases.
        const spellings = [member, member[0].toUpperCase() + member.slice(1)]
        expect(spellings.some((name) => code.includes(name))).toBe(true)
      }
      // No hand-written member table: the generator writes it, and a sample
      // showing one would be teaching the thing that was removed.
      expect(code).not.toContain('TacMember')
      expect(code).not.toContain('TacField')
    }
  })

  test('the published data is what the browser will fetch', async () => {
    const published = JSON.parse(await read('dist/web/shared/data/languages.json'))
    const authored = JSON.parse(await read('client/shared/data/languages.json'))
    expect(published).toEqual(authored)
  })
})

describe('discovery and metadata', () => {
  test('every page declares canonical and social metadata', async () => {
    // The metadata is declared once, in the configuration module, and the
    // compiler writes it into each document. A tac.html is a view now, so it
    // carries the content and nothing about the page it is rendered into.
    for (const [route, document] of [['/', 'index.html'], ['/docs', 'docs/index.html'],
                                     ['/docs/features', 'docs/features/index.html']]) {
      const html = await read(`dist/web/${document}`)
      // A page without these is invisible to a crawler and ugly when shared.
      expect(html).toContain('rel="canonical"')
      expect(html).toContain('property="og:title"')
      expect(html).toContain('name="twitter:card"')
      expect(html).toContain('lang="en"')
      expect(html).toContain(`href="https://tachyon.del.ma${route === '/' ? '/' : route}"`)
    }
    // The chrome is authored once. Navigation is cross-document, so every
    // *document* still carries its own header and footer; what the layout
    // removes is five copies of that in the source.
    const layout = await read('client/components/site/layout/tac.html')
    expect(layout).toContain('href="#main"')
    expect(layout).toContain('<site-header />')
    expect(layout).toContain('<site-footer />')
    for (const view of ['client/pages/tac.html', 'client/pages/docs/tac.html',
                        'client/pages/docs/features/tac.html']) {
      const authored = await read(view)
      expect(authored).toContain('<site-layout>')
      expect(authored).not.toContain('<site-header')
      expect(authored).not.toContain('<site-footer')
      // A view is not a document.
      expect(authored).not.toContain('<head>')
      expect(authored).not.toContain('<body>')
    }
    // And every emitted document still has the chrome and the skip target.
    for (const document of ['index.html', 'docs/index.html', 'docs/features/index.html']) {
      const html = await read(`dist/web/${document}`)
      expect(html).toContain('#main')
      expect(html).toContain('site-header')
      expect(html).toContain('site-footer')
    }
  })

  test('the build emits a sitemap covering every guide, and robots hides the API', async () => {
    const docs = JSON.parse(await read('client/shared/data/docs.json'))
    const sitemap = await read('dist/web/sitemap.xml')
    for (const slug of docs.order) expect(sitemap).toContain(`/docs/${slug}<`)
    // A dynamic route is a pattern, not a page.
    expect(sitemap).not.toContain('_topic')
    const robots = await read('dist/web/robots.txt')
    expect(robots).toContain('Disallow: /api/')
    expect(robots).toContain('Sitemap:')
  })
})

describe('server layer', () => {
  test('the Yon routes read the term from the path, not a query string', async () => {
    // Handler Protocol v1 carries no query string, so a search term has to be
    // a dynamic segment. Regressing this would silently return no results.
    const handler = await read('server/routes/api/search/_query/yon.js')
    expect(handler).toContain('parameters?.query')
    expect(handler).not.toContain('?q=')
  })

  test('a layer imports the Fylo shim relative to itself', async () => {
    // A handler is imported from its own path now, so a relative specifier
    // resolves the way it reads. This test previously asserted the opposite,
    // guarding a data: URL workaround that no longer applies.
    //
    // The import sits in the service rather than the route: a controller
    // answers HTTP methods and nothing else, so the searching moved down a
    // layer and took its dependency with it.
    const service = await read('server/services/search.js')
    expect(service).toContain("vendor/fylo.mjs'")
    expect(service).not.toContain('pathToFileURL')
    expect(service).not.toContain('process.cwd()')

    const handler = await read('server/routes/api/search/_query/yon.js')
    expect(handler).toContain('@Controller')
    expect(handler).toContain("services/search.js'")
  })
})

describe('feature catalogue', () => {
  test('covers every group and carries a snippet for each feature', async () => {
    const { features } = JSON.parse(await read('client/shared/data/features.json'))
    const groups = new Set(features.map((feature) => feature.group))
    for (const group of ['Routing', 'Tac views', 'Browser storage', 'Polyglot',
                         'Yon server', 'Native', 'Tooling']) {
      expect(groups).toContain(group)
    }
    // A feature without code is a claim, not documentation.
    for (const feature of features) {
      expect(feature.files.length).toBeGreaterThan(0)
      for (const file of feature.files) expect(file.code.length).toBeGreaterThan(10)
    }
    // Identifiers are anchors, so they have to stay unique.
    const ids = features.map((feature) => feature.id)
    expect(ids.length).toBe(new Set(ids).size)
  })

  test('documents the framework surface the ledger claims', async () => {
    const { features } = JSON.parse(await read('client/shared/data/features.json'))
    const text = JSON.stringify(features)
    // One probe per area the parity ledger records as supported, so a feature
    // cannot quietly drop off the page. `yon.rb` and `yon.rs` were probes here
    // until Yon narrowed to the languages that can declare a layer; they are
    // replaced by two that stayed rather than deleted, so the count of areas
    // covered does not quietly fall.
    for (const probe of ['_id', '<logic :if', '<loop :for', '<switch', '<slot',
                         'hydrate=', 'on:click', '$$theme', 'policy:', 'tachyon-sw',
                         'tac.rs', 'tac.swift', 'tac.kt', 'tac.cs',
                         'TY1010', 'yon.js', 'yon.py',
                         'yon.php', 'yon.kt', 'yon.java', 'middleware', 'EventSource', 'async *GET',
                         'companion.invoke', 'entryRoute', 'ty doctor',
                         'ty start', 'TY1307', 'TY1404',
                         'ty migrate check', 'postBundle', 'hotState', 'OPTIONS.schema.json']) {
      expect(text).toContain(probe)
    }
  })

  test('every published API route declares a contract', async () => {
    // The site is the framework's own showcase, so its routes are held to the
    // feature it documents: a caller can ask what they accept and answer.
    const { readdir } = await import('node:fs/promises')
    const routes = fileURLToPath(new URL('../server/routes', import.meta.url))
    const found = []
    const walk = async (directory) => {
      for (const entry of await readdir(directory, { withFileTypes: true })) {
        const child = `${directory}/${entry.name}`
        if (entry.isDirectory()) await walk(child)
        else if (entry.name.startsWith('yon.')) found.push(directory)
      }
    }
    await walk(routes)
    expect(found.length).toBeGreaterThan(0)
    for (const directory of found) {
      const contract = JSON.parse(await readFile(`${directory}/OPTIONS.schema.json`, 'utf8'))
      expect(Object.keys(contract.methods).length).toBeGreaterThan(0)
      // CHEX schematics: every leaf is a regular expression.
      const leaves = (value) => typeof value === 'string' ? [value]
        : Array.isArray(value) ? value.flatMap(leaves)
        : value && typeof value === 'object' ? Object.values(value).flatMap(leaves)
        : []
      for (const method of Object.values(contract.methods)) {
        for (const schema of [method.request?.body, method.request?.parameters,
                              method.request?.headers, method.ok, method.clientError]) {
          for (const pattern of leaves(schema)) {
            // A pattern that is not anchored matches a substring, which is
            // rarely what a contract means.
            expect(pattern.startsWith('^') && pattern.endsWith('$')).toBe(true)
            expect(() => new RegExp(pattern)).not.toThrow()
          }
        }
      }
    }
  })

  test('the removed releases page leaves nothing behind', async () => {
    const header = await read('client/components/site/header/tac.html')
    expect(header).not.toContain('/releases')
  })
})

describe('house style', () => {
  test('every page section uses the shared eyebrow, title and lede', async () => {
    // The three DELMA sites share this rhythm. Spelling it out per template is
    // how eight pages quietly become eight designs.
    const css = await read('client/shared/styles/site.css')
    for (const rule of ['.eyebrow', '.section-title', '.section-lede', '.panel', '.command', '.stat-value']) {
      expect(css).toContain(rule)
    }

    for (const component of ['home/hero', 'home/pillars', 'home/quickstart', 'home/faq']) {
      const html = await read(`client/components/${component}/tac.html`)
      expect(html).toContain('class="eyebrow"')
    }
  })

  test('a first visit lands on the dark theme', async () => {
    // DuVay falls back to light, and the palette is designed around dark, so
    // the seed is what stops a first paint looking like a different product.
    const imports = await read('client/shared/scripts/imports.js')
    expect(imports).toContain("localStorage.getItem('w-theme') ?? 'dark'")

    const css = await read('client/shared/styles/site.css')
    expect(css).toContain(':root[w-theme="dark"]')
    expect(css).toContain(':root[w-theme="light"]')
  })

  test('no page restates a number the data already carries', async () => {
    // The stats band is gone: a row of counts above the page that proves them
    // was saying the same thing twice.
    const { readdir } = await import('node:fs/promises')
    const components = await readdir(fileURLToPath(new URL('../client/components/site', import.meta.url)))
    expect(components).not.toContain('stats')
    const home = await read('client/pages/tac.html')
    expect(home).not.toContain('site-stats')
  })
})

describe('responsive system', () => {
  test('layout is driven by space, not by a breakpoint list', async () => {
    const css = await read('client/shared/styles/site.css')
    // Intrinsic grids and container queries, so a component is correct
    // wherever it is placed rather than only where it was authored.
    expect(css).toContain('repeat(auto-fit, minmax(min(100%, var(--col')
    expect(css).toContain('grid-auto-rows: 1fr')
    expect(css).toContain('container-type: inline-size')
    // The docs shell gains a column at each of these, measured against the
    // space it has rather than the window's width.
    expect(css).toContain('@container (min-width: 52rem)')
    expect(css).toContain('@container (min-width: 68rem)')
  })

  test('no template reintroduces a breakpoint grid', async () => {
    // A single `md:grid-cols-3` is enough to put one section back on cliffs
    // while every other section interpolates.
    for (const template of ['site/footer', 'home/quickstart']) {
      const html = await read(`client/components/${template}/tac.html`)
      expect(html).not.toMatch(/\b(sm|md|lg|xl):grid-cols-/)
      expect(html).toContain('grid-auto')
    }
    // The catalogue and the landing grid reflow on their own width, so
    // neither may reintroduce a breakpoint column count.
    for (const stylesheet of ['client/shared/styles/site.css',
                              'client/components/home/pillars/tac.css']) {
      const css = await read(stylesheet)
      expect(css).toContain('auto-fit')
    }
    // The catalogue is drawn by two components, so its rules are shared: a
    // component stylesheet is scoped to its own component, and colocating them
    // left the docs home with the class and none of the rules.
    expect(await read('client/shared/styles/site.css')).toContain('.catalogue__card')
  })

  test('the type scale interpolates rather than stepping', async () => {
    const css = await read('client/shared/styles/site.css')
    for (const step of ['--step-xs', '--step-sm', '--step-lg', '--step-2xl', '--gutter', '--section-space']) {
      expect(css).toMatch(new RegExp(`${step}: clamp\\(`))
    }
    // The vw term is what makes it a ramp; a clamp of two fixed values is
    // still a step, just a hidden one.
    expect(css).toMatch(/--step-2xl: clamp\([^)]*vw/)
  })

  test('the site answers to more than viewport width', async () => {
    const css = await read('client/shared/styles/site.css')
    expect(css).toContain('@media (pointer: coarse)')
    expect(css).toContain('@media (prefers-contrast: more)')
    expect(css).toContain('@media (prefers-reduced-motion: reduce)')

    // The docs shell gains its columns on the space it has rather than on the
    // window's width: it sits inside a container already narrower than the
    // viewport, so a viewport breakpoint drops the contents list while the
    // shell still has room for it.
    const shell = await read('client/shared/styles/site.css')
    expect(shell).toContain('.docs-frame')
    expect(shell).toContain('@container (min-width: 52rem)')
    expect(shell).toContain('@container (min-width: 68rem)')
    expect(shell).not.toMatch(/@media[^{]*max-width[^{]*\{\s*\.docs-shell/)
  })
})

describe('documentation structure', () => {
  const readJson = async (relative) => JSON.parse(await read(relative))

  test('every feature is a page of its own', async () => {
    // The explorer this replaces could show only the feature you had clicked
    // and could not be linked to, so every one of them was unreachable from a
    // search result or from anyone else's link.
    const routes = await import('node:fs/promises')
      .then(({ readdir }) => readdir(fileURLToPath(new URL('../client/pages/docs/features', import.meta.url))))
    expect(routes).toContain('tac.html')
    expect(routes).toContain('_id')

    const catalogue = await readJson('client/shared/data/features.json')
    // An id is a URL segment now, so it has to survive being one.
    for (const feature of catalogue.features) {
      expect(feature.id).toMatch(/^[a-z0-9]+(-[a-z0-9]+)*$/)
    }
    const ids = catalogue.features.map((feature) => feature.id)
    expect(new Set(ids).size).toBe(ids.length)
  })

  test('the sidebar reaches every page, and the pager passes through all of them', async () => {
    const navigation = await read('client/shared/scripts/navigation.js')
    const docs = await readJson('client/shared/data/docs.json')
    const catalogue = await readJson('client/shared/data/features.json')

    // The group order is the one list the data does not carry, so a group
    // added to the catalogue and not to it would be silently unreachable.
    const declared = [...navigation.matchAll(/^ {2}'(.+)',$/gm)].map((match) => match[1])
    const groups = [...new Set(catalogue.features.map((feature) => feature.group))]
    for (const group of groups) expect(declared).toContain(group)

    // One reading order across guides and features together: the last guide
    // leads into the first feature rather than into nothing.
    const total = docs.order.length + catalogue.features.length
    expect(total).toBeGreaterThan(docs.order.length)
  })

  test('the search index covers features, not only guides', async () => {
    // Every feature is a page now, so a search that could only land a reader
    // on /docs and leave them to hunt is a search that lost the change.
    const seed = await read('scripts/seed.mjs')
    const navigation = await read('client/shared/scripts/navigation.js')
    const search = await read('client/components/site/search/tac.js')
    expect(seed).toContain('features.json')
    expect(seed).toContain("kind: 'feature'")
    expect(seed).toContain('/docs/features/')
    expect(navigation).toContain('export const searchDocs')
    expect(navigation).toContain("kind: 'section'")
    expect(navigation).toContain("kind: 'feature'")
    expect(search).toContain('this.results = searchDocs(query)')
    expect(search).toContain('input.addEventListener')
    expect(search).toContain('keydown')
    expect(search).toContain('globalThis.__tachyonTac?.render()')
  })

  test('the sitemap lists the feature pages', async () => {
    const config = await read('tac.config.js')
    expect(config).toContain('/docs/features/${feature.id}')
    expect(config).toContain('"/docs/features"')
    expect(config).toContain('"/docs/features/_id"')
  })

  test('a diagnostic the site quotes still says what the code emits', async () => {
    // The site quotes diagnostics verbatim and nothing was checking the quote
    // kept up. TY2105's help changed when servers gained
    // TACHYON_JAVASCRIPT_RUNTIME and the page went on showing the old advice,
    // which is the failure every one of these quotes is one edit away from.
    //
    // The page wraps and indents for reading, the source wraps for line
    // length, and the source interpolates '{program}' where the page shows a
    // real one. So the page's help is split at its quoted values and each run
    // of prose between them has to appear in the source.
    //
    // Only the page is split that way. Rust spends single quotes on lifetimes
    // and apostrophes, so stripping them from the source swallowed whole
    // functions — which is how this test first passed while wrong.
    const flatten = (text) =>
      text
        .replace(/\\\s*\n\s*/g, '')   // Rust string continuations
        .replace(/\\"/g, '"')          // escaped quotes inside a Rust literal
        .replace(/\s+/g, ' ')
    const corpus = flatten(
      (await Promise.all([
        read('../crates/tachyon-core/src/handler/process.rs'),
        read('../crates/tachyon-core/src/html.rs'),
        read('../crates/tachyon-core/src/stereotype.rs'),
        read('../crates/tachyon-core/src/template/frontend.rs'),
        read('../crates/tachyon-core/src/project.rs').catch(() => ''),
        read('../crates/tachyon-core/src/native/routes.rs').catch(() => ''),
      ])).join('\n'),
    )

    // The diagnostics page and no other. Its whole purpose is to quote, so a
    // quote there has to be exact. Elsewhere a diagnostic is an illustration
    // with its placeholders filled — TY1010 interpolates a companion list and
    // a target name mid-sentence, and demanding an exact match of that would
    // be a test that fails for being right.
    const catalogue = JSON.parse(await read('client/shared/data/features.json'))
    const quoted = []
    for (const feature of catalogue.features.filter((one) => one.id === 'diagnostics')) {
      for (const file of feature.files) {
        for (const block of file.code.split(/\n\s*\n/)) {
          const code = block.match(/error\[(TY\d{4})\]/)?.[1]
          const help = block.split('help:')[1]
          if (code && help) quoted.push({ code, help: flatten(help).trim() })
        }
      }
    }
    expect(quoted.length).toBeGreaterThan(2)

    let checked = 0
    for (const { code, help } of quoted) {
      for (const fragment of help.split(/'[^']*'/).map((part) => part.trim())) {
        // A short run between two values is a joining word and proves nothing.
        if (fragment.length < 24) continue
        checked += 1
        expect(
          corpus.includes(fragment),
          `${code} on the site says "${fragment}", which is not what the code emits`,
        ).toBe(true)
      }
    }
    // A test that checked nothing would pass just as quietly.
    expect(checked).toBeGreaterThan(2)
  })

  test('a Tac example shows its view and its companion together', async () => {
    // A view and the companion behind it are two halves of one example. Shown
    // apart, a `{count}` in the view has no visible source and the field that
    // declares it has nothing to render — so every Tac feature carries both,
    // and `tac.css` when the example is about styling.
    const catalogue = JSON.parse(await read('client/shared/data/features.json'))
    const isCompanionFile = (name) => /\/tac\.[a-z]+$|^tac\.[a-z]+$/.test(name) && name !== 'tac.config.js'

    const tacFeatures = catalogue.features.filter((feature) =>
      feature.files.some((file) => isCompanionFile(file.name)))
    // The rest are shell commands, Yon handlers, configuration and diagrams:
    // a `ty doctor` example has no view, and inventing one would be a snippet
    // the gate then compiles as though it were real.
    expect(tacFeatures.length).toBeGreaterThan(15)

    for (const feature of tacFeatures) {
      const names = feature.files.map((file) => file.name)
      expect(names.some((name) => name.endsWith('tac.html'))).toBe(true)
      expect(names.some((name) => /tac\.(js|ts)$/.test(name))).toBe(true)
    }
  })

  test('an example reads behaviour, then structure, then presentation', async () => {
    const { inReadingOrder, rank } = await import('../client/shared/scripts/platforms.js')

    // The companion leads because it is the half that differs: a view and a
    // stylesheet are the same file on every target, so the thing a reader came
    // to compare should not sit below two files they have already read.
    expect(rank('tac.js')).toBeLessThan(rank('tac.html'))
    expect(rank('tac.html')).toBeLessThan(rank('tac.css'))
    // JavaScript before TypeScript: the one that needs no toolchain is where a
    // reader lands before choosing otherwise.
    expect(rank('tac.js')).toBeLessThan(rank('tac.ts'))
    expect(rank('tac.ts')).toBeLessThan(rank('tac.swift'))
    // A pseudo-file is the result of the example, not part of it.
    expect(rank('emitted')).toBeGreaterThan(rank('tac.css'))

    const shuffled = [{ name: 'emitted' }, { name: 'tac.css' }, { name: 'tac.html' }, { name: 'tac.swift' }]
    expect(inReadingOrder(shuffled).map((file) => file.name))
      .toEqual(['tac.swift', 'tac.html', 'tac.css', 'emitted'])
  })

  test('the framework layers are rendered together, not switched between', async () => {
    // Both cards exist and the toggle hides one. A component property is
    // assigned once, when the instance is created, and two <docs-snippet>
    // elements in mutually exclusive <logic> branches share a single instance
    // — so the branch that rendered first kept its files and the toggle
    // changed nothing but the label. This has now been the same bug twice, in
    // two shapes, which is why it is pinned here rather than only commented.
    const html = await read('client/components/home/languages/tac.html')
    expect(html).toContain(`:hidden="layer !== 'tac'"`)
    expect(html).toContain(`:hidden="layer !== 'yon'"`)
    // Neither card may sit in a branch: that is the arrangement that shares an
    // instance between them.
    expect(html).not.toMatch(/<logic[^>]*>\s*<docs-snippet/)
    expect(html).not.toContain('<logic else>')
  })

  test('the view and the styles are shown once, whatever the companion', async () => {
    // They are the same file on every target, so they are stored once and
    // rendered once: switching platform or companion swaps the companion block
    // and nothing else. A suite that repeated them would have the reader
    // re-reading two files to reach the one that changed.
    const { inReadingOrder, isCompanion } = await import('../client/shared/scripts/platforms.js')
    const languages = JSON.parse(await read('client/shared/data/languages.json'))
    const files = languages.tac.files

    expect(files.filter((file) => file.name.endsWith('.html'))).toHaveLength(1)
    expect(files.filter((file) => file.name.endsWith('.css'))).toHaveLength(1)

    // A view and a stylesheet are never companions, so they are never one of
    // the things the second strip chooses between.
    expect(isCompanion('client/pages/tac.html')).toBe(false)
    expect(isCompanion('client/pages/tac.css')).toBe(false)
    expect(isCompanion('client/pages/tac.swift')).toBe(true)

    // Behaviour first, then the two that do not change.
    const order = inReadingOrder(files).map((file) => file.name.split('/').pop())
    expect(order.at(-2)).toBe('tac.html')
    expect(order.at(-1)).toBe('tac.css')
    expect(isCompanion(order[0])).toBe(true)
  })

  test('a platform strip only appears where the example actually differs', async () => {
    const { spansPlatforms, filesFor, reachOf } =
      await import('../client/shared/scripts/platforms.js')

    // One companion language reaches every target on its own, so a strip over
    // a single tac.js would be six tabs that all do the same thing.
    expect(spansPlatforms([{ name: 'client/pages/tac.js' }])).toBe(false)
    expect(spansPlatforms([{ name: 'tac.js' }, { name: 'tac.swift' }])).toBe(true)

    // The reach is the companion matrix: Rust is desktop-only, Swift is Apple,
    // Kotlin is Android, C# is Windows.
    expect(reachOf('tac.rs')).toEqual(['macos', 'windows', 'linux'])
    expect(reachOf('tac.swift')).toEqual(['macos', 'ios'])
    expect(reachOf('tac.kt')).toEqual(['android'])
    expect(reachOf('tac.cs')).toEqual(['windows'])
    // A view is not a companion and belongs under every platform.
    expect(reachOf('tac.html')).toContain('android')

    const files = [{ name: 'tac.swift' }, { name: 'tac.kt' }, { name: 'tac.js' }]
    expect(filesFor(files, 'android').map((file) => file.name)).toEqual(['tac.kt', 'tac.js'])
    expect(filesFor(files, 'ios').map((file) => file.name)).toEqual(['tac.swift', 'tac.js'])
  })

  test('the navigation is a drawer where the shell has no column for it', async () => {
    // Stacked in flow it pushed the article a whole screen down, so every
    // documentation page opened on its own navigation rather than on what the
    // reader came for.
    const css = await read('client/components/docs/sidebar/tac.css')
    expect(css).toContain('@container (max-width: 51.999rem)')
    // Closed, it is out of the tab order as well as off the screen: a drawer
    // that is hidden but still focusable is a list of links a keyboard walks
    // into from nowhere.
    expect(css).toContain('visibility: hidden')

    // A closed <details> hides its content through the UA's own display rule,
    // and an author `display` on a direct child overrides it — which left a
    // "collapsed" section fully reachable by keyboard and to a screen reader.
    expect(css).toContain('.docs-nav__section[open] > .docs-nav__list')
    expect(css).toContain('.docs-nav__section:not([open]) > .docs-nav__list')

    const html = await read('client/components/docs/sidebar/tac.html')
    expect(html).toContain('aria-expanded')
    expect(html).toContain('aria-controls="docs-nav-panel"')
    const source = await read('client/components/docs/sidebar/tac.js')
    // Escape closes it, which is what any overlay owes a keyboard.
    expect(source).toContain("key === 'Escape'")
  })

  test('the docs shell dissolves its wrappers where the rule can win', async () => {
    // site.css is @layer brand and the runtime's rule for a mounted component
    // is unlayered, so `display: contents` there loses whatever its
    // specificity. It has to be colocated, which is unlayered.
    const layout = await read('client/components/docs/layout/tac.css')
    expect(layout).toContain('display: contents')
    expect(layout).toContain('data-tachyon-component')
    const site = await read('client/shared/styles/site.css')
    expect(site).not.toMatch(/^\.docs-page \{\n\s*display: contents/m)
  })
})
