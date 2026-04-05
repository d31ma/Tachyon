import Router from "../server/route-handler.js";
import { BunRequest } from 'bun';
import { exists } from 'node:fs/promises';

interface ParsedElement {
    static?: string;
    element?: string;
}

interface ComponentData {
    html: string;
    script?: string;
    scriptLang?: string;
}

const TEMPLATE_PATH = `${import.meta.dir}/render-template.js`;
const ROUTES_JSON_PATH = `${import.meta.dir}/../runtime/route-manifest.json`;
const LAYOUTS_JSON_PATH = `${import.meta.dir}/../runtime/layout-manifest.json`;
const NOT_FOUND_PATH = `${import.meta.dir}/../runtime/shells/not-found.html`;

const jsResponse = (body: BodyInit) =>
    new Response(body, { headers: { 'Content-Type': 'application/javascript' } });

const jsonResponse = (path: string) =>
    async () => new Response(await Bun.file(path).bytes(), { headers: { 'Content-Type': 'application/json' } });

export default class Yon {

    private static readonly htmlMethod = 'HTML'
    private static readonly layoutMethod = 'LAYOUT'
    private static readonly compMapping = new Map<string, string>()
    private static layoutMapping: Record<string, string> = {}

    static getParams(request: BunRequest, route: string) {
        const url = new URL(request.url)
        const params = url.pathname.split('/').slice(route.split('/').length)
        return { params: Router.parseParams(params) }
    }

    static async createStaticRoutes() {
        Yon.compMapping.clear()
        Yon.layoutMapping = {}

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
                    return `${name}="\${ty_invokeEvent('${hash}', '${value}')}"`;
                if (name === ':value')
                    return `value="\${ty_assignValue('${hash}', '${value}')}"`;
                return `${name}="${value}"`;
            };

            const interpolate = (text: string) =>
                text.replace(/\{([^{}]+)\}/g, '${$1}').replace(/\{\{([^{}]+)\}\}/g, '{${$1}}');

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

                        // Component import
                        if (tag.endsWith('_')) {
                            const compName = tagLower.slice(0, -1);
                            const filepath = Yon.compMapping.get(compName);
                            const isLazy = 'lazy' in attrs;

                            if (filepath && compName && !isLazy) {
                                const existing = imports.get(filepath);
                                if (!existing || !existing.has(compName)) {
                                    const keyword = existing ? 'const' : 'const';
                                    const awaitPrefix = existing ? '' : 'await ';
                                    parsed.push({ static: `${keyword} { default: ${compName} } = ${awaitPrefix}import('/components/${filepath}')` });
                                    if (existing) existing.add(compName);
                                    else imports.set(filepath, new Set([compName]));
                                }
                            }
                        }

                        // Auto-generate id for non-control, non-component elements
                        if (!attrs.id && !tag.endsWith('_') && tag !== 'LOOP' && tag !== 'LOGIC') {
                            attrs[':id'] = `ty_generateId('${hash}', 'id')`;
                        }

                        const attrStr = Object.entries(attrs).map(([n, v]) => formatAttr(n, v, hash)).join(' ');
                        tagStack.push(tagLower);

                        if (element.selfClosing) {
                            parsed.push({ element: `\`<${tagLower} ${attrStr} />\`` });
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
                    /<([A-Za-z0-9-]+)_\s+([^/>]*)\/>/.test(el.element)) {
                    body.push(el.element);
                } else {
                    body.push(`elements+=${el.element}`);
                }
            }
        }

        let code = await Bun.file(TEMPLATE_PATH).text();

        code = code
            .replaceAll('// imports', statics.join('\n'))
            .replaceAll('// script', scriptContent ?? '')
            .replaceAll('// inners', body.join('\n'));

        // Transform control flow tags to JS
        code = code
            .replaceAll(/`<loop :for="(.*?)">`|`<\/loop>`/g, (_, expr) => expr ? `for(${expr}) {` : '}')
            .replaceAll(/`<logic :if="(.*?)">`|`<\/logic>`/g, (_, expr) => expr ? `if(${expr}) {` : '}')
            .replaceAll(/`<logic :else-if="(.*?)">`|`<\/logic>`/g, (_, expr) => expr ? `else if(${expr}) {` : '}')
            .replaceAll(/`<logic else="">`|`<\/logic>`/g, (_, expr) => expr ? `else {` : '}');

        // Bind dynamic attributes :attr="expr" → attr="${expr}"
        code = code.replaceAll(/:(\w[\w-]*)="([^"]*)"/g, '$1="${$2}"');

        // Transform component invocations
        code = code.replaceAll(/`<([A-Za-z0-9-]+)_\s+([^/>]*)\/>`/g, (_, component, attrStr) => {
            const matches = attrStr.matchAll(/([a-zA-Z0-9-@]+)="([^"]*)"/g);
            const props: string[] = [];
            const events: string[] = [];
            const hash = genHash();
            const isLazy = /\blazy\b/.test(attrStr);

            for (const [, key, value] of matches) {
                if (key === 'lazy') continue;
                if (key.startsWith('@')) {
                    events.push(`${key}="${value.replace(/(ty_invokeEvent\(')([^"]+)(',[^)]+\))/g, `$1${hash}$3`)}"`);
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
                    render = await ${component}(${propsObj})
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

    // ── Build & register a single template module ──────────────────────────────
    private static async registerModule(data: ComponentData, route: string, dir: 'pages' | 'components' | 'layouts') {
        const parsed = await Yon.parseHTML(data.html);
        const jsCode = await Yon.createJSData(parsed, data.script);

        const srcRoute = route.replace('.html', `.${data.scriptLang || 'js'}`);
        const tmpPath = `/tmp/${srcRoute}`;

        await Bun.write(Bun.file(tmpPath), jsCode);

        const result = await Bun.build({
            entrypoints: [tmpPath],
            external: ["*"],
            minify: { whitespace: true, syntax: true }
        });

        const outRoute = srcRoute.replace('.ts', '.js');

        Router.reqRoutes[`/${dir}/${outRoute}`] = {
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
                await Router.validateRoute(route);
                const data = await Yon.extractComponents(await Bun.file(`${Router.routesPath}/${route}`).text());
                await Yon.registerModule(data, `${route}.${data.scriptLang || 'js'}`, 'pages');
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
            const prefix = layout === Yon.layoutMethod ? '/' : `/${layout.replace(`/${Yon.layoutMethod}`, '')}`;
            const data = await Yon.extractComponents(await Bun.file(`${Router.routesPath}/${layout}`).text());
            const layoutRoute = layout === Yon.layoutMethod ? Yon.layoutMethod : layout;
            await Yon.registerModule(data, `${layoutRoute}.${data.scriptLang || 'js'}`, 'layouts');
            Yon.layoutMapping[prefix] = `/layouts/${layoutRoute}.js`;
        }
    }

    private static async bundleComponents() {
        if (!await exists(Router.componentsPath)) return;

        for (const comp of new Bun.Glob('**/*.html').scanSync({ cwd: Router.componentsPath })) {
            const filename = comp.split('/').pop()!.replace('.html', '');
            Yon.compMapping.set(filename, comp.replace('.html', '.js'));
            const data = await Yon.extractComponents(await Bun.file(`${Router.componentsPath}/${comp}`).text());
            await Yon.registerModule(data, comp, 'components');
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
                console.warn(`Failed to bundle module '${mod}': ${(e as Error).message}`, process.pid);
            }
        }
    }
}

function genHash(): string {
    return Bun.randomUUIDv7().replace(/-/g, '').slice(-8);
}