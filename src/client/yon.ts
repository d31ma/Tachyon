import Router from "../router.js";
import { BunRequest } from 'bun';
import { exists } from 'node:fs/promises';

interface ParsedElement {
    static?: string;
    render?: string;
    element?: string;
}

interface ComponentData {
    html: string;
    script?: string;
    scriptLang?: string;
}

export default class Yon {

    private static htmlMethod = 'HTML'

    private static compMapping = new Map<string, string>()

    static getParams(request: BunRequest, route: string) {
        
        const url = new URL(request.url)

        const params = url.pathname.split('/').slice(route.split('/').length)

        return { params: Router.parseParams(params) }
    }

    static async createStaticRoutes() {

        const result = await Bun.build({
            entrypoints: [`${import.meta.dir}/render.ts`, `${import.meta.dir}/hmr.ts`],
            minify: true
        })

        for(const output of result.outputs) {

            Router.reqRoutes[output.path.replace('./', '/')] = {
                GET: async () => new Response(output, { headers: { 'Content-Type': 'application/javascript' } })
            }
        }

        Router.reqRoutes["/routes.json"] = {
            GET: async () => new Response(await Bun.file(`${import.meta.dir}/routes.json`).bytes(), { headers: { 'Content-Type': 'application/json' } })
        }

        const main = Bun.file(`${process.cwd()}/main.js`)

        if(await main.exists()) {
            Router.reqRoutes["/main.js"] = {
                GET: async () => new Response(await main.bytes(),  { headers: { 'Content-Type': 'application/javascript' } })
            }
        }
        
        await Promise.all([Yon.bundleDependencies(), Yon.bundleComponents(), Yon.bundlePages(), Yon.bundleAssets()])
        
        await Bun.write(Bun.file(`${import.meta.dir}/routes.json`), JSON.stringify(Router.routeSlugs))
    }

    private static async extractComponents(data: string): Promise<ComponentData> {
        let htmlContent = '';
        let scriptContent = '';
        let scriptLang = 'js';

        const rewriter = new HTMLRewriter()
            .on('script', {
                element(element) {
                    const lang = element.getAttribute('lang');
                    if (lang) {
                        scriptLang = lang;
                    }
                },
                text(text) {
                    scriptContent += text.text;
                }
            })
            .on('*', {
                element(element) {
                    if (element.tagName !== 'script') {
                        // Keep non-script elements for HTML content
                        element.onEndTag(() => {
                            // This will be handled by the final transform
                        });
                    } else {
                        // Remove script tags from HTML content
                        element.remove();
                    }
                }
            });

        // First pass: extract script content and remove script tags
        const withoutScripts = rewriter.transform(new Response(data));
        htmlContent = await withoutScripts.text();

        return {
            html: htmlContent,
            script: scriptContent || undefined,
            scriptLang
        };
    }

    private static parseHTML(
        htmlContent: string,
        imports: Map<string, Set<string>> = new Map<string, Set<string>>()
    ): Promise<Array<ParsedElement>> {
        
        return new Promise((resolve) => {
            const parsed: Array<ParsedElement> = [];
            const elementStack: Array<{tagName: string, hash: string, attributes: string[]}> = [];
            
            const parseAttrs = (attrs: {[key: string]: string}, hash: string): string[] => {
                return Object.entries(attrs).map(([name, value]) => {
                    if(name.startsWith('@')) {
                        return `${name}="` + "${eval(ty_invokeEvent('" + hash + "', '" + value + "'))}" + '"'
                    }

                    if(name === ":value") {
                        return `${name.replace(':', '')}="` + "${eval(ty_assignValue('" + hash + "', '" + value + "'))}" + '"'
                    }
                    
                    return `${name}="${value}"`
                });
            };

            const interpolateText = (textContext: string) => 
                textContext.replace(/\{([^{}]+)\}/g, '${$1}').replace(/\{\{([^{}]+)\}\}/g, '{${$1}}');

            const rewriter = new HTMLRewriter()
                .on('script', {
                    element(element) {
                        element.remove();
                    }
                })
                .on('style', {
                    element(element) {
                        // Handle style elements
                    },
                    text(text) {
                        parsed.push({ element: `\`<style>@scope { ${text.text} }</style>\`` });
                    }
                })
                .on('*', {
                    element(element) {
                        const tagName = element.tagName.toUpperCase();
                        
                        if (tagName === 'SCRIPT' || tagName === 'STYLE') {
                            return;
                        }

                        const hash = Bun.randomUUIDv7().split('-')[3];
                        const attributes: Record<string, string> = {}

                        Array.from(element.attributes).map(attr => {
                            const [name, value] = attr
                            attributes[name] = value
                        })

                        // Handle custom attributes (this is a limitation of HTMLRewriter)
                        // You might need to parse these manually from the original HTML

                        if(tagName.startsWith('TY') && !tagName.endsWith('LOOP') && !tagName.endsWith('LOGIC')) {
                            const component = tagName.split('-')[1]?.toLowerCase();
                            const filepath = Yon.compMapping.get(component || '');

                            if(filepath && component) {
                                if(imports.has(filepath)) {
                                    if(!imports.get(filepath)?.has(component)) {
                                        parsed.push({ static: `const { default: ${component} } = import('/components/${filepath}')`});
                                        imports.get(filepath)?.add(component);
                                    }
                                } else {
                                    parsed.push({ static: `const { default: ${component} } = await import('/components/${filepath}')`});
                                    imports.set(filepath, new Set<string>([component]));
                                }
                            }
                        }

                        if(!attributes.id && !tagName.startsWith('TY-')) {
                            attributes[':id'] = "ty_generateId('" + hash + "', 'id')";
                        }

                        const parsedAttrs = parseAttrs(attributes, hash);
                        elementStack.push({tagName: tagName.toLowerCase(), hash, attributes: parsedAttrs});

                        if (element.selfClosing) {
                            parsed.push({ element: `\`<${tagName.toLowerCase()} ${parsedAttrs.join(" ")} />\`` });
                            elementStack.pop();
                        } else {
                            parsed.push({ element: `\`<${tagName.toLowerCase()} ${parsedAttrs.join(" ")}>\`` });
                        }
                    },
                    text(text) {
                        if (text.text.trim()) {
                            parsed.push({ element: `\`${interpolateText(text.text)}\`` });
                        }
                    }
                })
                .on('*', {
                    element(element) {
                        element.onEndTag(() => {
                            const current = elementStack.pop();
                            if (current && !element.selfClosing) {
                                parsed.push({ element: `\`</${current.tagName}>\`` });
                            }
                        });
                    }
                });

            rewriter.transform(new Response(htmlContent)).text().then(() => {
                resolve(parsed);
            });
        });
    }

    private static async createJSData(html: ParsedElement[], scriptContent?: string) {

        const inners: string[] = []
        const outers: string[] = []

        html.forEach(h => {
            if(h.element) {
                if(h.element.includes('<ty-') || h.element.includes('</ty-')) {
                    inners.push(h.element)
                } else inners.push(`elements+=${h.element}`)
            }
            if(h.static) outers.push(h.static)
        })

        const tempFile = await Bun.file(`${import.meta.dir}/template.js`).text()
        
        return tempFile.replaceAll('// imports', outers.join('\n'))
                        .replaceAll('// script', scriptContent ?? '')
                        .replaceAll('// inners', inners.join('\n'))
                        .replaceAll(/`<ty-loop :for="(.*?)">`|`<\/ty-loop>`/g, (match, p1) => {
                            if(p1) return `for(${p1}) {`
                            else return '}'
                        })
                        .replaceAll(/`<ty-logic :if="(.*?)">`|`<\/ty-logic>`/g, (match, p1) => {
                            if(p1) return `if(${p1}) {`
                            else return '}'
                        })
                        .replaceAll(/`<ty-logic :else-if="(.*?)">`|`<\/ty-logic>`/g, (match, p1) => {
                            
                            if(p1) return `else if(${p1}) {`
                            else return '}'
                        })
                        .replaceAll(/`<ty-logic :else="">`|`<\/ty-logic>`/g, (match, p1) => {
                            if(p1) return `else {`
                            else return '}'
                        })
                        .replaceAll(/:([^"]*)="([^"]*)"/g, '$1="${$2}"')
                        .replaceAll(/`<\/ty-(\w+)\s*>`/g, '')
                        .replaceAll(/`<ty-([a-zA-Z0-9-]+)(?:\s+([^>]*))>`/g, (match, component, atrributes) => {
                         
                            const matches = atrributes.matchAll(/([a-zA-Z0-9-@]+)="([^"]*)"/g)

                            const props: string[] = []
                            const events: string[] = []

                            const hash = Bun.randomUUIDv7().split('-')[3]
                            
                            for(const [_, key, value] of matches) {
                                if(key.startsWith('@')) events.push(`${key}="${value.replace(/(ty_invokeEvent\(')([^"]+)(',[^)]+\))/g, `$1${hash}$3`)}"`)
                                else props.push(`${key}=${value}`)
                            }

                            const genId = "${ty_generateId('" + hash + "', 'id')}"
                            
                            return `
                                elements += \`<div id="${genId}" ${events.join(' ')}>\`

                                if(!compRenders.has('${hash}')) {
                                    render = await ${component}(\`${props.join(';')}\`)
                                    elements += await render(elemId, event, '${hash}')
                                    compRenders.set('${hash}', render)
                                } else {
                                    render = compRenders.get('${hash}')
                                    elements += await render(elemId, event, '${hash}')
                                }

                                elements += '</div>'
                            `
                        })
                        
    }

    private static async addToStatix(componentData: ComponentData, route: string, dir: 'pages' | 'components') {
        
        const module = await Yon.parseHTML(componentData.html)

        const jsData = await Yon.createJSData(module, componentData.script)

        route = route.replace('.html', `.${componentData.scriptLang || 'js'}`)

        await Bun.write(Bun.file(`/tmp/${route}`), jsData)

        const result = await Bun.build({
            entrypoints: [`/tmp/${route}`],
            external: ["*"],
            minify: {
                whitespace: true,
                syntax: true
            }
        })

        route = route.replace('.ts', '.js')

        Router.reqRoutes[`/${dir}/${route}`] = {
            GET: () => new Response(result.outputs[0], { headers: { 'Content-Type': 'application/javascript' } })
        }
    }

    private static async bundleAssets() {

        if(await exists(Router.assetsPath)) {

            const routes = Array.from(new Bun.Glob(`**/*`).scanSync({ cwd: Router.assetsPath }))

            for(const route of routes) {

                const file = Bun.file(`${Router.assetsPath}/${route}`)

                Router.reqRoutes[`/assets/${route}`] = {
                    GET: async () => new Response(await file.bytes(), { headers: { 'Content-Type': file.type }})
                }
            }
        }
    }

    private static async bundlePages() {

        if(await exists(Router.routesPath)) {

            const routes = Array.from(new Bun.Glob(`**/${Yon.htmlMethod}`).scanSync({ cwd: Router.routesPath }))
        
            for(const route of routes) {

                await Router.validateRoute(route)

                const data = await Bun.file(`${Router.routesPath}/${route}`).text()

                const componentData = await Yon.extractComponents(data)
                
                await Yon.addToStatix(componentData, `${route}.${componentData.scriptLang || 'js'}`, 'pages')
            }
        }

        const nfFile = Bun.file(`${process.cwd()}/404.html`)

        const data = await nfFile.exists() ? await nfFile.text() : await Bun.file(`${import.meta.dir}/404.html`).text()

        const componentData = await Yon.extractComponents(data)
        
        await Yon.addToStatix(componentData, '404.html', 'pages')
    }

    private static async bundleComponents() {

        if(await exists(Router.componentsPath)) {

            const components = Array.from(new Bun.Glob(`**/*.html`).scanSync({ cwd: Router.componentsPath }))

            for(let comp of components) {

                const folders = comp.split('/')

                const filename = folders[folders.length - 1].replace('.html', '')

                Yon.compMapping.set(filename, comp.replace('.html', '.js'))

                const data = await Bun.file(`${Router.componentsPath}/${comp}`).text()

                const componentData = await Yon.extractComponents(data)
                
                await Yon.addToStatix(componentData, comp, 'components')
            }
        }
    }

    private static async bundleDependencies() {

        const packageFile = Bun.file(`${process.cwd()}/package.json`)

        const otherEntries = ['index.js', 'index', 'index.node']
        
        if(await packageFile.exists()) {

            const packages = await packageFile.json()

            const modules = Object.keys(packages.dependencies ?? {})

            for(const module of modules) {

                let modPack = await Bun.file(`${process.cwd()}/node_modules/${module}/package.json`).json()
                
                let idx = 0
                let entryExists = false
                
                while(!modPack.main && !entryExists && idx < otherEntries.length) {

                    entryExists = await Bun.file(`${process.cwd()}/node_modules/${module}/${otherEntries[idx]}`).exists()

                    if(entryExists) {
                        modPack.main = otherEntries[idx]
                        break
                    }
 
                    idx++
                }

                if(!modPack.main) continue
                
                try {

                    const result = await Bun.build({
                        entrypoints: [`${process.cwd()}/node_modules/${module}/${(modPack.main as string).replace('./', '')}`],
                        minify: true
                    })

                    for(const output of result.outputs) {
                        Router.reqRoutes[`/modules/${module}.js`] = {
                            GET: () => new Response(output, { headers: { 'Content-Type': 'application/javascript' } })
                        }
                    }

                } catch(e) {}
            }
        }
    }
}