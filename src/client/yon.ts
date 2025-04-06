import { JSDOM } from 'jsdom'
import Router from "../router.js";
import { BunRequest } from 'bun';
import { exists } from 'node:fs/promises';

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

    private static extractComponents(data: string) {

        const html = new JSDOM('').window.document.createElement('div')

        html.innerHTML = data

        return { html, script:  html.querySelectorAll('script')[0] }
    }

    private static parseHTML(
        elements: HTMLCollection,
        imports: Map<string, Set<string>> = new Map<string, Set<string>>()
    ): Array<{ static?: string; render?: string; element?: string }> {
        
        const parsed: Array<{ static?: string; render?: string; element?: string }> = [];
        
        const parseAttrs = (attrs: NamedNodeMap, hash: string) => Array.from(attrs).map(attr => {
            
            if(attr.name.startsWith('@')) {
                return `${attr.name}="` + "${eval(ty_invokeEvent('" + hash + "', '" + attr.value + "'))}" + '"'
            }

            if(attr.name === ":value") {
                return `${attr.name.replace(':', '')}="` + "${eval(ty_assignValue('" + hash + "', '" + attr.value + "'))}" + '"'
            }
            
            return `${attr.name}="${attr.value}"`
        })

        const interpolateText = (textContext: string) => textContext.replace(/\{([^{}]+)\}/g, '${$1}').replace(/\{\{([^{}]+)\}\}/g, '{${$1}}')
        
        for (const element of Array.from(elements)) {

            if (element.tagName === "SCRIPT") {
                continue; // Skip script tags as they're handled separately
            }

            if(element.tagName === 'STYLE') {
                element.innerHTML = `@scope { ${element.innerHTML} }`
                parsed.push({ element: `\`${element.outerHTML}\`` })
                continue
            }

            if(element.tagName.startsWith('TY') && !element.tagName.endsWith('LOOP') && !element.tagName.endsWith('LOGIC')) {
                
                const component = element.tagName.split('-')[1].toLowerCase()

                const filepath = Yon.compMapping.get(component)

                if(filepath) {

                    if(imports.has(filepath)) {
    
                        if(!imports.get(filepath)?.has(component)) {
                            parsed.push({ static: `const { default: ${component} } = import('/components/${filepath}')`})
                            imports.get(filepath)?.add(component)
                        }

                    } else {

                        parsed.push({ static: `const { default: ${component} } = await import('/components/${filepath}')`})
                        imports.set(filepath, new Set<string>([component]))
                    }
                }
            }

            const hash = Bun.randomUUIDv7().split('-')[3]

            if(!element.id && !element.tagName.startsWith('TY-')) {
                element.setAttribute(':id', "ty_generateId('" + hash + "', 'id')")
            }
            
            if(element.children.length > 0) {

                const text = Array.from(element.childNodes).reduce((a, b) => {
                    return a + (b.nodeType === 3 ? b.textContent : '')
                }, '')

                parsed.push({ element: `\`<${element.tagName.toLowerCase()} ${parseAttrs(element.attributes, hash).join(" ")}>\`` })
                if(text) parsed.push({ element: `\`${interpolateText(text)}\`` })
                parsed.push(...this.parseHTML(element.children, imports))
                parsed.push({ element: `\`</${element.tagName.toLowerCase()}>\`` })

            } else {

                if(element.outerHTML.includes('</')) {
                    parsed.push({ element: `\`<${element.tagName.toLowerCase()} ${parseAttrs(element.attributes, hash).join(" ")}>\`` })
                    if(element.textContent) parsed.push({ element: `\`${interpolateText(element.textContent)}\`` })
                    parsed.push({ element: `\`</${element.tagName.toLowerCase()}>\`` })
                } else {
                    parsed.push({ element: `\`<${element.tagName.toLowerCase()} ${parseAttrs(element.attributes, hash).join(" ")} />\`` })
                }
            }
        }
    
        return parsed;
    }

    private static async createJSData(html: { static?: string, render?: string, element?: string }[], scriptTag?: HTMLScriptElement) {

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
                        .replaceAll('// script', scriptTag?.innerHTML ?? '')
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
                         
                            const matches = atrributes.matchAll(/([a-zA-Z0-9-]+)="([^"]*)"/g)

                            const exports: string[] = []
                            
                            for(const [_, key, value] of matches) {
                                exports.push(`${key}=${value}`)
                            }
                            
                            const hash = Bun.randomUUIDv7().split('-')[3]
                            
                            return `
                                elements += '<div>'

                                if(!compRenders.has('${hash}')) {
                                    render = await ${component}(\`${exports.join(';')}\`)
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

    private static async addToStatix(html: HTMLDivElement, script: HTMLScriptElement, route: string, dir: 'pages' | 'components') {
        
        const module = Yon.parseHTML(html.children)

        const jsData = await Yon.createJSData(module, script)

        route = route.replace('.html', `.${script?.lang || 'js'}`)

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

                const { html, script } = Yon.extractComponents(data)
                
                await Yon.addToStatix(html, script, `${route}.${script?.lang || 'js'}`, 'pages')
            }
        }

        const nfFile = Bun.file(`${process.cwd()}/404.html`)

        const data = await nfFile.exists() ? await nfFile.text() : await Bun.file(`${import.meta.dir}/404.html`).text()

        const { html, script } = Yon.extractComponents(data)
        
        await Yon.addToStatix(html, script, '404.html', 'pages')
    }

    private static async bundleComponents() {

        if(await exists(Router.componentsPath)) {

            const components = Array.from(new Bun.Glob(`**/*.html`).scanSync({ cwd: Router.componentsPath }))

            for(let comp of components) {

                const folders = comp.split('/')

                const filename = folders[folders.length - 1].replace('.html', '')

                Yon.compMapping.set(filename, comp.replace('.html', '.js'))

                const data = await Bun.file(`${Router.componentsPath}/${comp}`).text()

                const { html, script } = Yon.extractComponents(data)
                
                await Yon.addToStatix(html, script, comp, 'components')
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