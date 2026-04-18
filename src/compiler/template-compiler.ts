import Router from "../server/route-handler.js";
import logger from "../server/logger.js";
import { BunRequest } from 'bun';
import { exists, mkdir, readFile, writeFile } from 'node:fs/promises';
import path from "node:path";
import { pathToFileURL } from "node:url";

interface ParsedElement {
    static?: string;
    element?: string;
}

interface ComponentData {
    html: string;
    script?: string;
    scriptLang?: string;
}

type ElementTagResolution =
    | { kind: 'component'; name: string }
    | { kind: 'web-component' }
    | { kind: 'native' }
    | { kind: 'control' }
    | { kind: 'unknown' }

type YonOutputFormat = 'esm' | 'global'

const TEMPLATE_PATH = `${import.meta.dir}/render-template.js`;
const ROUTES_JSON_PATH = `${import.meta.dir}/../runtime/route-manifest.json`;
const LAYOUTS_JSON_PATH = `${import.meta.dir}/../runtime/layout-manifest.json`;
const NOT_FOUND_PATH = `${import.meta.dir}/../runtime/shells/not-found.html`;
const PRODUCTION_SHELL_PATH = `${import.meta.dir}/../runtime/shells/production.html`;
const PRERENDER_WORKER_PATH = `${import.meta.dir}/../runtime/prerender-worker.ts`;

const jsResponse = (body: BodyInit) =>
    new Response(body, { headers: { 'Content-Type': 'application/javascript' } });

const jsonResponse = (path: string) =>
    async () => new Response(await Bun.file(path).bytes(), { headers: { 'Content-Type': 'application/json' } });

export default class Yon {
    private static readonly compilerLogger = logger.child({ scope: 'compiler' })

    private static readonly htmlMethod = 'HTML'
    private static readonly layoutMethod = 'LAYOUT'
    private static readonly prerenderRenderConcurrency = 4
    private static readonly prerenderWriteConcurrency = 8
    private static readonly outputFormat = Yon.resolveOutputFormat()
    private static readonly compMapping = new Map<string, string>()
    private static layoutMapping: Record<string, string> = {}
    private static readonly warnedUnknownTags = new Set<string>()
    private static readonly nativeTags = new Set([
        'a', 'abbr', 'address', 'area', 'article', 'aside', 'audio', 'b', 'base', 'bdi', 'bdo',
        'blockquote', 'body', 'br', 'button', 'canvas', 'caption', 'cite', 'code', 'col',
        'colgroup', 'data', 'datalist', 'dd', 'del', 'details', 'dfn', 'dialog', 'div', 'dl',
        'dt', 'em', 'embed', 'fieldset', 'figcaption', 'figure', 'footer', 'form', 'h1', 'h2',
        'h3', 'h4', 'h5', 'h6', 'head', 'header', 'hgroup', 'hr', 'html', 'i', 'iframe', 'img',
        'input', 'ins', 'kbd', 'label', 'legend', 'li', 'link', 'main', 'map', 'mark', 'menu',
        'meta', 'meter', 'nav', 'noscript', 'object', 'ol', 'optgroup', 'option', 'output', 'p',
        'picture', 'pre', 'progress', 'q', 'rp', 'rt', 'ruby', 's', 'samp', 'script', 'search',
        'section', 'select', 'slot', 'small', 'source', 'span', 'strong', 'style', 'sub',
        'summary', 'sup', 'table', 'tbody', 'td', 'template', 'textarea', 'tfoot', 'th', 'thead',
        'time', 'title', 'tr', 'track', 'u', 'ul', 'var', 'video', 'wbr',
        'svg', 'g', 'path', 'circle', 'ellipse', 'line', 'polygon', 'polyline', 'rect', 'defs',
        'lineargradient', 'radialgradient', 'stop', 'symbol', 'use', 'clippath', 'mask', 'text',
        'tspan', 'foreignobject', 'animate', 'animatemotion', 'animatetransform', 'pattern',
        'marker', 'filter', 'feblend', 'fecolormatrix', 'fecomponenttransfer', 'fecomposite',
        'feconvolvematrix', 'fediffuselighting', 'fedisplacementmap', 'fedistantlight',
        'fedropshadow', 'feflood', 'fefunca', 'fefuncb', 'fefuncg', 'fefuncr', 'fegaussianblur',
        'feimage', 'femerge', 'femergenode', 'femorphology', 'feoffset', 'fepointlight',
        'fespecularlighting', 'fespotlight', 'fetile', 'feturbulence'
    ])
    private static readonly controlTags = new Set(['loop', 'logic'])
    private static readonly reservedCustomElementNames = new Set([
        'annotation-xml',
        'color-profile',
        'font-face',
        'font-face-src',
        'font-face-uri',
        'font-face-format',
        'font-face-name',
        'missing-glyph',
    ])

    private static readonly routeTitleFallback = "Tachyon"

    private static resolveOutputFormat(): YonOutputFormat {
        const value = (process.env.YON_FORMAT || process.env.TACHYON_YON_FORMAT || 'esm').toLowerCase()
        return value === 'global' || value === 'umd' || value === 'iife' ? 'global' : 'esm'
    }

    private static isGlobalOutput() {
        return Yon.outputFormat === 'global'
    }

    static getParams(request: BunRequest, route: string) {
        const url = new URL(request.url)
        const params = url.pathname.split('/').slice(route.split('/').length)
        return { params: Router.parseParams(params) }
    }

    private static normalizeComponentName(route: string): string {
        const segments = route.replace('.html', '').split('/').filter(Boolean)
        if (segments[segments.length - 1] === 'index') segments.pop()
        return segments.join('-').toLowerCase()
    }

    private static classifyElementTag(tagName: string): ElementTagResolution {
        const normalized = tagName.toLowerCase()

        if (normalized.endsWith('_')) {
            const legacyName = normalized.slice(0, -1)
            return Yon.compMapping.has(legacyName)
                ? { kind: 'component', name: legacyName }
                : { kind: 'unknown' }
        }

        if (Yon.compMapping.has(normalized)) return { kind: 'component', name: normalized }
        if (Yon.isWebComponentTag(normalized)) return { kind: 'web-component' }
        if (Yon.nativeTags.has(normalized)) return { kind: 'native' }
        if (Yon.controlTags.has(normalized)) return { kind: 'control' }

        return { kind: 'unknown' }
    }

    private static isWebComponentTag(tagName: string): boolean {
        return /^[a-z][.0-9_a-z]*-[.0-9_a-z-]*$/.test(tagName)
            && !Yon.reservedCustomElementNames.has(tagName)
    }

    private static warnUnknownTag(tagName: string, sourceName: string) {
        const key = `${sourceName}:${tagName}`
        if (Yon.warnedUnknownTags.has(key)) return

        Yon.warnedUnknownTags.add(key)
        Yon.compilerLogger.warn('Unknown element tag; treating it as plain HTML', {
            tag: tagName,
            source: sourceName,
        })
    }

    static async createStaticRoutes() {
        Yon.compMapping.clear()
        Yon.layoutMapping = {}
        Yon.warnedUnknownTags.clear()

        // Build client-side render + HMR scripts
        const result = await Bun.build({
            entrypoints: [`${import.meta.dir}/../runtime/spa-renderer.ts`, `${import.meta.dir}/../runtime/hot-reload-client.ts`],
            minify: true
        })

        for (const output of result.outputs) {
            Router.reqRoutes[output.path.replace('./', '/')] = {
                GET: async () => jsResponse(output)
            }
        }

        // JSON manifests
        Router.reqRoutes["/routes.json"] = { GET: jsonResponse(ROUTES_JSON_PATH) }
        Router.reqRoutes["/layouts.json"] = { GET: jsonResponse(LAYOUTS_JSON_PATH) }

        // Optional user main.js
        const main = Bun.file(`${process.cwd()}/main.js`)
        if (await main.exists()) {
            Router.reqRoutes["/main.js"] = {
                GET: async () => new Response(await main.bytes(), { headers: { 'Content-Type': 'application/javascript' } })
            }
        }

        // Components must be registered before pages/layouts compile so that
        // compMapping is fully populated when import statements are generated.
        await Yon.bundleComponents()

        await Promise.all([
            Yon.bundleDependencies(),
            Yon.bundleLayouts(),
            Yon.bundlePages(),
            Yon.bundleAssets()
        ])

        // Write manifests after all routes are registered
        await Promise.all([
            Bun.write(Bun.file(ROUTES_JSON_PATH), JSON.stringify(Router.routeSlugs)),
            Bun.write(Bun.file(LAYOUTS_JSON_PATH), JSON.stringify(Yon.layoutMapping))
        ])
    }

    private static normalizeScopedStyles(html: string) {
        return html.replace(/<style>([\s\S]*?)<\/style>/g, (_match, css: string) => {
            const trimmed = css.trim();

            if (trimmed === "@scope {  }" || trimmed === "@scope {}") {
                return "";
            }

            if (trimmed.startsWith("@scope {") && trimmed.endsWith("}")) {
                const inner = trimmed.slice("@scope {".length, -1).trim();
                return `<style>${inner}</style>`;
            }

            return `<style>${css}</style>`;
        });
    }

    private static escapeHTML(value: string) {
        return value
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;")
            .replaceAll('"', "&quot;")
            .replaceAll("'", "&#39;");
    }

    private static resolveLayout(pathname: string): string | null {
        if (pathname !== '/') {
            const segments = pathname.split('/').filter(Boolean)
            for (let count = segments.length; count > 0; count -= 1) {
                const prefix = `/${segments.slice(0, count).join('/')}`
                if (Yon.layoutMapping[prefix]) return Yon.layoutMapping[prefix]
            }
        }

        return Yon.layoutMapping['/'] ?? null
    }

    private static async rewriteAbsoluteImports(filePath: string, distPath: string) {
        const source = await readFile(filePath, 'utf8')
        const fileDir = path.dirname(filePath)

        const rewritten = source.replace(
            /import\("(?<spec>\/(?:components|modules)\/[^"]+)"\)/g,
            (_match, spec: string) => {
                const relative = path.relative(fileDir, path.join(distPath, spec.slice(1))).replaceAll(path.sep, '/')
                const normalized = relative.startsWith('.') ? relative : `./${relative}`
                return `import("${normalized}")`
            }
        )

        if (rewritten !== source) await writeFile(filePath, rewritten)
    }

    private static installPrerenderGlobals(distPath: string) {
        const previousDocument = globalThis.document
        const previousWindow = globalThis.window
        const previousYon = (globalThis as Record<string, unknown>).Yon
        const modules = new Map<string, (props?: unknown) => Promise<(elementId?: string | null, eventDetail?: unknown) => Promise<string>>>()

        const titleState = { value: '' }
        const documentStub = {
            title: '',
            head: {
                appendChild() { return null }
            },
            body: {
                appendChild() { return null }
            },
            documentElement: {
                dataset: {} as Record<string, string>,
                style: {} as Record<string, string>,
                classList: {
                    add() {},
                    remove() {},
                    toggle() { return false }
                },
                setAttribute(name: string, value: string) {
                    this.dataset[name] = value
                },
                removeAttribute(name: string) {
                    delete this.dataset[name]
                }
            },
            createElement() {
                return {
                    setAttribute() {},
                    appendChild() {},
                    rel: '',
                    type: '',
                    href: ''
                }
            }
        }

        Object.defineProperty(documentStub, 'title', {
            configurable: true,
            enumerable: true,
            get() {
                return titleState.value
            },
            set(value: string) {
                titleState.value = String(value)
            }
        })

        Object.assign(globalThis, {
            document: documentStub,
            window: globalThis,
            Yon: {
                version: '1',
                modules,
                register(id: string, factory: (props?: unknown) => Promise<(elementId?: string | null, eventDetail?: unknown) => Promise<string>>) {
                    modules.set(id, factory)
                    return factory
                },
                async load(id: string) {
                    const registered = modules.get(id)
                    if (registered) return registered

                    const relative = id.startsWith('/') ? id.slice(1) : id
                    const modulePath = pathToFileURL(path.join(distPath, relative)).href
                    const mod = await import(modulePath)
                    const loaded = modules.get(id)
                    if (loaded) return loaded
                    if (typeof mod.default === 'function') return mod.default

                    throw new Error(`Yon module '${id}' did not export or register a renderer`)
                }
            }
        })

        return {
            get title() {
                return titleState.value
            },
            restore() {
                if (previousDocument === undefined) Reflect.deleteProperty(globalThis as Record<string, unknown>, 'document')
                else Object.assign(globalThis, { document: previousDocument })

                if (previousWindow === undefined) Reflect.deleteProperty(globalThis as Record<string, unknown>, 'window')
                else Object.assign(globalThis, { window: previousWindow })

                if (previousYon === undefined) Reflect.deleteProperty(globalThis as Record<string, unknown>, 'Yon')
                else Object.assign(globalThis, { Yon: previousYon })
            }
        }
    }

    private static async loadRenderFactoryForPrerender(filePath: string, publicPath: string) {
        const mod = await import(pathToFileURL(filePath).href)
        if (typeof mod.default === 'function') return mod.default

        const yon = (globalThis as {
            Yon?: {
                modules?: Map<string, (props?: unknown) => Promise<(elementId?: string | null, eventDetail?: unknown) => Promise<string>>>
            }
        }).Yon
        const registered = yon?.modules?.get(publicPath)
        if (registered) return registered

        throw new Error(`Yon module '${publicPath}' did not export or register a renderer`)
    }

    private static async renderPageDocument(
        distPath: string,
        pathname: string,
        shellHTML: string
    ) {
        const pageFile = pathname === '/'
            ? path.join(distPath, 'pages', 'HTML.js')
            : path.join(distPath, 'pages', pathname.slice(1), 'HTML.js')
        const pagePublicPath = pathname === '/'
            ? '/pages/HTML.js'
            : `/pages/${pathname.slice(1)}/HTML.js`

        await Yon.rewriteAbsoluteImports(pageFile, distPath)

        const layoutRoute = Yon.resolveLayout(pathname)
        const layoutFile = layoutRoute
            ? path.join(distPath, layoutRoute.slice(1))
            : null

        if (layoutFile) await Yon.rewriteAbsoluteImports(layoutFile, distPath)

        const prerender = Yon.installPrerenderGlobals(distPath)

        try {
            const pageModule = await Yon.loadRenderFactoryForPrerender(pageFile, pagePublicPath)
            const pageFactory = await pageModule()
            const pageHTML = await pageFactory()

            let layoutHTML = ''

            if (layoutFile) {
                const layoutModule = await Yon.loadRenderFactoryForPrerender(layoutFile, layoutRoute!)
                const layoutFactory = await layoutModule()
                layoutHTML = await layoutFactory()
            }

            const bodyHTML = layoutHTML
                ? layoutHTML.replace('<div id="ty-layout-slot"></div>', `<div id="ty-layout-slot">${pageHTML}</div>`)
                : pageHTML

            const title = Yon.escapeHTML(prerender.title || Yon.routeTitleFallback)
            const withTitle = shellHTML.replace(/<title>.*?<\/title>/, `<title>${title}</title>`)
            const withBody = withTitle.replace('<body></body>', `<body>${bodyHTML}</body>`)
            return `${Yon.normalizeScopedStyles(withBody)}\n`
        } finally {
            prerender.restore()
        }
    }

    static async renderPageDocumentForWorker(
        distPath: string,
        pathname: string,
        shellHTML: string,
        layoutMapping: Record<string, string>
    ) {
        const previousLayoutMapping = Yon.layoutMapping
        Yon.layoutMapping = { ...layoutMapping }

        try {
            return await Yon.renderPageDocument(distPath, pathname, shellHTML)
        } finally {
            Yon.layoutMapping = previousLayoutMapping
        }
    }

    private static async renderPageDocumentIsolated(
        distPath: string,
        pathname: string,
        shellHTML: string
    ) {
        const proc = Bun.spawn(['bun', PRERENDER_WORKER_PATH], {
            cwd: process.cwd(),
            stdin: 'pipe',
            stdout: 'pipe',
            stderr: 'pipe',
        })

        proc.stdin.write(JSON.stringify({
            distPath,
            pathname,
            shellHTML,
            layoutMapping: Yon.layoutMapping,
        }))
        proc.stdin.end()

        const [stdout, stderr, exitCode] = await Promise.all([
            new Response(proc.stdout).text(),
            new Response(proc.stderr).text(),
            proc.exited,
        ])

        if (exitCode !== 0) {
            throw new Error(stderr.trim() || `Prerender worker exited with code ${exitCode}`)
        }

        return stdout
    }

    static async prerenderStaticPages(distPath: string) {
        const htmlRoutes = Yon.getHtmlRoutes()
        await Yon.prerenderRoutes(distPath, htmlRoutes)
    }

    static getHtmlRoutes() {
        return Array
            .from(Router.allRoutes.entries())
            .filter(([, methods]) => methods.has(Yon.htmlMethod))
            .map(([route]) => route)
    }

    static async prerenderRoutes(distPath: string, routes: string[]) {
        const shellHTML = await Bun.file(PRODUCTION_SHELL_PATH).text()
        const renderedRoutes: Array<{ outputFile: string; html: string }> = []
        let renderIndex = 0
        const renderWorkers = Array.from(
            { length: Math.min(Yon.prerenderRenderConcurrency, routes.length) },
            async () => {
                while (renderIndex < routes.length) {
                    const currentIndex = renderIndex++
                    const route = routes[currentIndex]
                    const outputFile = route === '/'
                        ? path.join(distPath, 'index.html')
                        : path.join(distPath, route.slice(1), 'index.html')

                    const html = routes.length === 1
                        ? await Yon.renderPageDocument(distPath, route, shellHTML)
                        : await Yon.renderPageDocumentIsolated(distPath, route, shellHTML)

                    renderedRoutes[currentIndex] = { outputFile, html }
                }
            }
        )

        await Promise.all(renderWorkers)

        let index = 0
        const workers = Array.from(
            { length: Math.min(Yon.prerenderWriteConcurrency, renderedRoutes.length) },
            async () => {
                while (index < renderedRoutes.length) {
                    const current = renderedRoutes[index++]
                    await mkdir(path.dirname(current.outputFile), { recursive: true })
                    await writeFile(current.outputFile, current.html)
                }
            }
        )

        await Promise.all(workers)
    }

    static async bundlePageFile(route: string) {
        await Router.validateRoute(route)
        const data = await Yon.extractComponents(await Bun.file(`${Router.routesPath}/${route}`).text())
        await Yon.registerModule(data, `${route}.${data.scriptLang || 'js'}`, 'pages')
    }

    static async bundleLayoutFile(layout: string) {
        const prefix = layout === Yon.layoutMethod ? '/' : `/${layout.replace(`/${Yon.layoutMethod}`, '')}`
        const data = await Yon.extractComponents(await Bun.file(`${Router.routesPath}/${layout}`).text())
        const layoutRoute = layout === Yon.layoutMethod ? Yon.layoutMethod : layout
        await Yon.registerModule(data, `${layoutRoute}.${data.scriptLang || 'js'}`, 'layouts')
        Yon.layoutMapping[prefix] = `/layouts/${layoutRoute}.js`
    }

    static async bundleComponentFile(comp: string) {
        const componentName = Yon.normalizeComponentName(comp)
        const modulePath = comp.replace('.html', '.js')

        if (Yon.compMapping.has(componentName) && Yon.compMapping.get(componentName) !== modulePath) {
            throw new Error(`Duplicate component name '${componentName}' for '${comp}' and '${Yon.compMapping.get(componentName)}'`)
        }

        Yon.compMapping.set(componentName, modulePath)
        const data = await Yon.extractComponents(await Bun.file(`${Router.componentsPath}/${comp}`).text())
        await Yon.registerModule(data, comp, 'components')
    }

    static async bundleAssetFile(route: string) {
        const file = Bun.file(`${Router.assetsPath}/${route}`)
        Router.reqRoutes[`/assets/${route}`] = {
            GET: async () => new Response(await file.bytes(), { headers: { 'Content-Type': file.type } })
        }
    }

    // ── Template extraction ────────────────────────────────────────────────────
    private static async extractComponents(data: string): Promise<ComponentData> {
        let scriptContent = '';
        let scriptLang = 'js';

        const rewriter = new HTMLRewriter()
            .on('script', {
                element(element) {
                    const lang = element.getAttribute('lang');
                    if (lang) scriptLang = lang;
                },
                text(text) { scriptContent += text.text; }
            })

        const htmlContent = await rewriter.transform(new Response(data)).text();

        return {
            html: htmlContent,
            script: scriptContent || undefined,
            scriptLang
        };
    }

    // ── HTML → AST parsing ─────────────────────────────────────────────────────
    private static parseHTML(
        htmlContent: string,
        sourceName = 'template',
        imports: Map<string, Set<string>> = new Map()
    ): Promise<ParsedElement[]> {
        return new Promise((resolve) => {
            const parsed: ParsedElement[] = [];
            const tagStack: string[] = [];
            let insideScript = false;
            let insideStyle = false;

            const genHash = () => Bun.randomUUIDv7().replace(/-/g, '').slice(-8);

            const formatAttr = (name: string, value: string, hash: string): string => {
                if (name.startsWith('@'))
                    return `${name}="\${await ty_invokeEvent('${hash}', ($event) => { const __event__ = $event; ${value} })}"`;
                if (name === ':value')
                    return `value="\${ty_assignValue('${hash}', '${value}')}"`;
                return `${name}="${value}"`;
            };

            const interpolate = (text: string) => {
                const rawExpressions: string[] = [];

                return text
                    .replace(/\{!\s*([^{}]+?)\s*\}/g, (_match, expr) => {
                        rawExpressions.push(expr);
                        return `__TY_RAW_${rawExpressions.length - 1}__`;
                    })
                    .replace(/\{\s*([^{}!][^{}]*?)\s*\}/g, (_match, expr) => `\${ty_escapeText(${expr})}`)
                    .replace(/__TY_RAW_(\d+)__/g, (_match, index) => `\${${rawExpressions[Number(index)]}}`);
            };

            const rewriter = new HTMLRewriter()
                .on('script', {
                    element(el) {
                        insideScript = true;
                        el.onEndTag(() => { insideScript = false; });
                    }
                })
                .on('style', {
                    element(el) {
                        insideStyle = true;
                        el.onEndTag(() => { insideStyle = false; });
                    },
                    text(text) {
                        parsed.push({ element: `\`<style>@scope { ${text.text} }</style>\`` });
                    }
                })
                .on('*', {
                    element(element) {
                        const tag = element.tagName.toUpperCase();

                        if (tag === 'SCRIPT' || tag === 'STYLE') return;

                        if (tag === 'SLOT') {
                            parsed.push({ element: '`<div id="ty-layout-slot"></div>`' });
                            element.remove();
                            return;
                        }

                        const hash = genHash();
                        const tagLower = element.tagName.toLowerCase();
                        const attrs: Record<string, string> = {};

                        for (const [name, value] of element.attributes) attrs[name] = value;

                        const tagResolution = Yon.classifyElementTag(tagLower);
                        const resolvedComponent = tagResolution.kind === 'component' ? tagResolution.name : null;

                        if (tagResolution.kind === 'unknown') {
                            Yon.warnUnknownTag(tagLower, sourceName);
                        }

                        // Component import
                        if (resolvedComponent) {
                            const filepath = Yon.compMapping.get(resolvedComponent);
                            const isLazy = 'lazy' in attrs;

                            if (filepath && !isLazy) {
                                const existing = imports.get(filepath);
                                if (!existing || !existing.has(resolvedComponent)) {
                                    const keyword = existing ? 'const' : 'const';
                                    if (Yon.isGlobalOutput()) {
                                        parsed.push({ static: `${keyword} ${resolvedComponent.replaceAll('-', '_')} = await globalThis.Yon.load('/components/${filepath}')` });
                                    } else {
                                        const awaitPrefix = existing ? '' : 'await ';
                                        parsed.push({ static: `${keyword} { default: ${resolvedComponent.replaceAll('-', '_')} } = ${awaitPrefix}import('/components/${filepath}')` });
                                    }
                                    if (existing) existing.add(resolvedComponent);
                                    else imports.set(filepath, new Set([resolvedComponent]));
                                }
                            }
                        }

                        // Auto-generate id for non-control, non-component elements
                        if (!attrs.id && !resolvedComponent && tag !== 'LOOP' && tag !== 'LOGIC') {
                            attrs[':id'] = `ty_generateId('${hash}', 'id')`;
                        }

                        const attrStr = Object.entries(attrs).map(([n, v]) => formatAttr(n, v, hash)).join(' ');
                        tagStack.push(tagLower);

                        if (element.selfClosing) {
                            const tagName = resolvedComponent ? `${resolvedComponent}_` : tagLower;
                            parsed.push({ element: `\`<${tagName} ${attrStr} />\`` });
                            tagStack.pop();
                        } else {
                            parsed.push({ element: `\`<${tagLower} ${attrStr}>\`` });
                        }
                    },
                    text(text) {
                        if (text.text.trim() && !insideScript && !insideStyle) {
                            parsed.push({ element: `\`${interpolate(text.text)}\`` });
                        }
                    }
                })
                .on('*', {
                    element(element) {
                        if (element.selfClosing) return;
                        const tag = element.tagName.toUpperCase();
                        if (tag === 'SCRIPT' || tag === 'STYLE') return;
                        element.onEndTag(() => {
                    const tagName = tagStack.pop();
                            if (tagName) parsed.push({ element: `\`</${tagName}>\`` });
                        });
                    }
                });

            rewriter.transform(new Response(htmlContent)).text().then(() => resolve(parsed));
        });
    }

    // ── JS code generation ─────────────────────────────────────────────────────
    private static async createJSData(elements: ParsedElement[], scriptContent?: string): Promise<string> {
        const statics: string[] = [];
        const body: string[] = [];

        for (const el of elements) {
            if (el.static) statics.push(el.static);
            if (el.element) {
                // Control flow and component tags are raw JS, not concatenated
                if (el.element.includes('<loop') || el.element.includes('</loop') ||
                    el.element.includes('<logic') || el.element.includes('</logic') ||
                    /<([A-Za-z0-9-]+)_\s*([\s\S]*?)\/>/.test(el.element)) {
                    body.push(el.element);
                } else {
                    body.push(`elements+=${el.element}`);
                }
            }
        }

        let code = await Bun.file(TEMPLATE_PATH).text();

        // Use split/join instead of replaceAll to avoid $ being interpreted
        // as special replacement patterns (e.g. $` inserts pre-match string)
        code = code.split('// imports').join(statics.join('\n'));
        code = code.split('// script').join(scriptContent ?? '');
        code = code.split('// inners').join(body.join('\n'));

        // Transform control flow tags to JS
        code = code
            .replaceAll(/`<loop :for="(.*?)">`|`<\/loop>`/g, (_, expr) => expr ? `for(${expr}) {` : '}')
            .replaceAll(/`<logic :if="(.*?)">`|`<\/logic>`/g, (_, expr) => expr ? `if(${expr}) {` : '}')
            .replaceAll(/`<logic :else-if="(.*?)">`|`<\/logic>`/g, (_, expr) => expr ? `else if(${expr}) {` : '}')
            .replaceAll(/`<logic else="">`|`<\/logic>`/g, (_, expr) => expr ? `else {` : '}');

        // Bind dynamic attributes :attr="expr" → attr="${escaped expr}"
        code = code.replaceAll(/:(\w[\w-]*)="([^"]*)"/g, '$1="${ty_escapeAttr($2)}"');

        // Transform component invocations
        code = code.replaceAll(/`<([A-Za-z0-9-]+)_\s*([\s\S]*?)\/>`/g, (_, component, attrStr) => {
            const matches = attrStr.matchAll(/([a-zA-Z0-9-@]+)="([^"]*)"/g);
            const props: string[] = [];
            const events: string[] = [];
            const hash = genHash();
            const isLazy = /\blazy\b/.test(attrStr);
            const renderName = component.replaceAll('-', '_');

            for (const [, key, value] of matches) {
                if (key === 'lazy') continue;
                if (key.startsWith('@')) {
                    events.push(`${key}="${value.replace(/(ty_invokeEvent\(')([^']+)(')/g, `$1${hash}$3`)}"`);
                } else {
                    // Convert template-literal expr "${foo()}" → foo(), static value → JSON literal
                    const expr = value.startsWith('${') && value.endsWith('}')
                        ? value.slice(2, -1)
                        : JSON.stringify(value);
                    props.push(`"${key}": ${expr}`);
                }
            }

            const genId = "${ty_generateId('" + hash + "', 'id')}";
            const propsObj = props.length ? `{${props.join(', ')}}` : 'null';

            if (isLazy) {
                const filepath = Yon.compMapping.get(component);
                return `
                elements += \`<div id="${genId}" data-lazy-component="${component}" data-lazy-path="/components/${filepath}" data-lazy-props="\${${props.length ? `encodeURIComponent(JSON.stringify(${propsObj}))` : "''"}}\" ${events.join(' ')}></div>\`
                `;
            }

            return `
                elements += \`<div id="${genId}" ${events.join(' ')}>\`
                if(!compRenders.has('${hash}')) {
                    render = await ${renderName}(${propsObj})
                    elements += await render(elemId, event, '${hash}')
                    compRenders.set('${hash}', render)
                } else {
                    render = compRenders.get('${hash}')
                    elements += await render(elemId, event, '${hash}')
                }
                elements += '</div>'
            `;
        });

        return code;
    }

    private static wrapGlobalModule(code: string, publicPath: string) {
        const factory = code.replace(/^export default\s+/, '')

        return `
(function(root, factory) {
    function ensureYon() {
        var existing = root.Yon || {};
        var modules = existing.modules || new Map();

        existing.version = existing.version || '1';
        existing.modules = modules;
        existing.register = existing.register || function(path, rendererFactory) {
            modules.set(path, rendererFactory);
            return rendererFactory;
        };
        existing.load = existing.load || async function(path) {
            var registered = modules.get(path);
            if (registered) return registered;

            var mod = await import(path);
            if (typeof mod.default === 'function') return mod.default;

            registered = modules.get(path);
            if (registered) return registered;

            throw new Error('Yon module "' + path + '" did not export or register a renderer');
        };

        root.Yon = existing;
        return existing;
    }

    ensureYon().register(${JSON.stringify(publicPath)}, factory);
})(globalThis, ${factory});
`
    }

    // ── Build & register a single template module ──────────────────────────────
    private static async registerModule(data: ComponentData, route: string, dir: 'pages' | 'components' | 'layouts') {
        const parsed = await Yon.parseHTML(data.html, `${dir}/${route}`);
        const srcRoute = route.replace('.html', `.${data.scriptLang || 'js'}`);
        const outRoute = srcRoute.replace('.ts', '.js');
        const publicPath = `/${dir}/${outRoute}`;
        const baseCode = await Yon.createJSData(parsed, data.script);
        const jsCode = Yon.isGlobalOutput()
            ? Yon.wrapGlobalModule(baseCode, publicPath)
            : baseCode;
        const tmpPath = path.join('/tmp', `tachyon-yon-${Bun.randomUUIDv7()}`, srcRoute);

        await mkdir(path.dirname(tmpPath), { recursive: true });
        await Bun.write(Bun.file(tmpPath), jsCode);

        if (Yon.isGlobalOutput()) {
            const loader = srcRoute.endsWith('.ts') ? 'ts' : 'js'
            const output = new Bun.Transpiler({ loader }).transformSync(jsCode)

            Router.reqRoutes[publicPath] = {
                GET: () => jsResponse(output)
            };
            return
        }

        const result = await Bun.build({
            entrypoints: [tmpPath],
            external: ["*"],
            minify: { whitespace: true, syntax: true }
        });

        Router.reqRoutes[publicPath] = {
            GET: () => jsResponse(result.outputs[0])
        };
    }

    // ── Asset bundlers ─────────────────────────────────────────────────────────
    private static async bundleAssets() {
        if (!await exists(Router.assetsPath)) return;

        for (const route of new Bun.Glob('**/*').scanSync({ cwd: Router.assetsPath })) {
            const file = Bun.file(`${Router.assetsPath}/${route}`);
            Router.reqRoutes[`/assets/${route}`] = {
                GET: async () => new Response(await file.bytes(), { headers: { 'Content-Type': file.type } })
            };
        }
    }

    private static async bundlePages() {
        if (await exists(Router.routesPath)) {
            for (const route of new Bun.Glob(`**/${Yon.htmlMethod}`).scanSync({ cwd: Router.routesPath })) {
                await Yon.bundlePageFile(route);
            }
        }

        // 404 page
        const nfFile = Bun.file(`${process.cwd()}/404.html`);
        const nfContent = await nfFile.exists()
            ? await nfFile.text()
            : await Bun.file(NOT_FOUND_PATH).text();
        const nfData = await Yon.extractComponents(nfContent);
        await Yon.registerModule(nfData, '404.html', 'pages');
    }

    private static async bundleLayouts() {
        if (!await exists(Router.routesPath)) return;

        for (const layout of new Bun.Glob(`**/${Yon.layoutMethod}`).scanSync({ cwd: Router.routesPath })) {
            await Yon.bundleLayoutFile(layout);
        }
    }

    private static async bundleComponents() {
        if (!await exists(Router.componentsPath)) return;

        for (const comp of new Bun.Glob('**/*.html').scanSync({ cwd: Router.componentsPath })) {
            await Yon.bundleComponentFile(comp);
        }
    }

    private static async bundleDependencies() {
        const packageFile = Bun.file(`${process.cwd()}/package.json`);
        if (!await packageFile.exists()) return;

        const packages = await packageFile.json();
        const modules = Object.keys(packages.dependencies ?? {});
        const fallbackEntries = ['index.js', 'index', 'index.node'];

        for (const mod of modules) {
            const modPackPath = `${process.cwd()}/node_modules/${mod}/package.json`;
            const modPack = await Bun.file(modPackPath).json();

            if (!modPack.main) {
                for (const entry of fallbackEntries) {
                    if (await Bun.file(`${process.cwd()}/node_modules/${mod}/${entry}`).exists()) {
                        modPack.main = entry;
                        break;
                    }
                }
            }

            if (!modPack.main) continue;

            try {
                const result = await Bun.build({
                    entrypoints: [`${process.cwd()}/node_modules/${mod}/${(modPack.main as string).replace('./', '')}`],
                    minify: true
                });

                for (const output of result.outputs) {
                    Router.reqRoutes[`/modules/${mod}.js`] = {
                        GET: () => jsResponse(output)
                    };
                }
            } catch (e) {
                Yon.compilerLogger.warn('Failed to bundle dependency module', { module: mod, err: e });
            }
        }
    }
}

function genHash(): string {
    return Bun.randomUUIDv7().replace(/-/g, '').slice(-8);
}
