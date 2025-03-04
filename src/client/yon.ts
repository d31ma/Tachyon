import { JSDOM } from 'jsdom'
import Router from "../router.js";
import { EventEmitter } from 'node:stream';
import { BunRequest } from 'bun';
import { exists } from 'node:fs/promises';

export default class Yon {

    private static htmlMethod = 'HTML'

    private static emitter = new EventEmitter()

    private static compMapping = new Map<string, string>()

    static getParams(request: BunRequest, route: string) {
        
        const url = new URL(request.url)

        const params = url.pathname.split('/').slice(route.split('/').length)

        return { params: Router.parseParams(params) }
    }

    static async createStaticRoutes() {

        Router.reqRoutes["/render.js"] = {
            GET: async () => new Response(await Bun.file(`${import.meta.dir}/render.js`).bytes(), { headers: { 'Content-Type': 'application/javascript' } })
        }

        Router.reqRoutes["/hmr.js"] = {
            GET: async () => new Response(await Bun.file(`${import.meta.dir}/hmr.js`).bytes(), { headers: { 'Content-Type': 'application/javascript' } })
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

        let styles = ''
        
        Yon.emitter.addListener('style', (msg) => {
            styles += `${msg}\n`
        })
        
        await Promise.all([Yon.bundleDependencies(), Yon.bundleComponents(), Yon.bundlePages(), Yon.bundleAssets()])
        
        await Bun.write(Bun.file(`${import.meta.dir}/routes.json`), JSON.stringify(Router.routeSlugs))
        
        Yon.emitter.removeAllListeners('style')
        
        if(styles) {
            Router.reqRoutes["/styles.css"] = {
                GET: () => new Response(styles, { headers: { 'Content-Type': 'text/css' }})
            }
        }
    }

    private static extractComponents(data: string) {

        const html = new JSDOM('').window.document.createElement('div')

        html.innerHTML = data

        const scripts = html.querySelectorAll('script')

        const script = scripts[0]

        scripts.forEach(s => s.remove())

        const styles = html.querySelectorAll('style')

        const style = styles[0]

        styles.forEach(s => s.remove())

        return { html, script, style }
    }

    private static parseHTML(elements: HTMLCollection, imports: Map<string, Set<string>> = new Map<string, Set<string>>()) {

        const parsed: { static?: string, render?: string, element?: string }[] = []

        for (const element of elements) {

            if(element.tagName.startsWith('TY-')) {

                const component = element.tagName.split('-')[1].toLowerCase()

                if(component === 'loop') {
                    const attribute = element.attributes[0];
                    if (attribute.name === ':for') parsed.push({ render: `for(${attribute.value}) {`})
                } else if(component === "logic") {
                    const attribute = element.attributes[0]
                    if (attribute.name === ':if') parsed.push({ render: `if(${attribute.value}) {`});
                    if (attribute.name === ':else-if') parsed.push({ render: `else if(${attribute.value}) {`});
                    if (attribute.name === ':else') parsed.push({ render: `else {`});
                } else {

                    const exports: string[] = []

                    const filepath = Yon.compMapping.get(component)

                    if(filepath) {

                        for(let i = 0; i < element.attributes.length; i++) {

                            if(element.attributes[i].name.startsWith(':')) {
                                const propName = element.attributes[i].name.slice(1)
                                exports.push(`${propName} = ${"${" + element.attributes[i].value + "}"}`)
                            } else {
                                const propName = element.attributes[i].name
                                exports.push(`${propName} = "${element.attributes[i].value}"`)
                            }
                        }

                        if(imports.has(filepath)) {

                            if(!imports.get(filepath)?.has(component)) {
                                parsed.push({ static: `const { default: ${component} } = import('/components/${filepath}')`})
                                imports.get(filepath)?.add(component)
                            }

                        } else {

                            parsed.push({ static: `const { default: ${component} } = await import('/components/${filepath}')`})
                            imports.set(filepath, new Set<string>([component]))
                        }

                        const hash = Bun.randomUUIDv7().split('-')[1]

                        parsed.push({ static: `const comp_${hash} = await ${component}(\`${exports.join(';')}\`)`})

                        parsed.push({ render: `elements += comp_${hash}(execute && execute.compId === "ty-${hash}" ? execute : null).replaceAll('class="', 'class="ty-${hash} ')`})
                    }
                }

                const temp = new JSDOM('').window.document.createElement('div');
                temp.innerHTML = element.innerHTML

                parsed.push(...this.parseHTML(temp.children, imports))

                if(component === "loop" || component === "logic") parsed.push({ render: '}'})

            } else {

                for(let i = 0; i < element.attributes.length; i++) {

                    const attr = element.attributes[i]

                    if(attr.name.startsWith(':')) {

                        const attrName = attr.name.slice(1)

                        element.removeAttribute(attr.name)
                        element.setAttribute(attrName, "${" + attr.value + "}")
                    }
                }

                parsed.push({ element: `\`${element.outerHTML}\`` })
            }
        }

        return parsed
    }

    private static createJSData(html: { static?: string, render?: string, element?: string }[], scriptTag?: HTMLScriptElement, style?: HTMLStyleElement) {

        const hash = Bun.randomUUIDv7().split('-')[3]

        if(style && style.innerHTML) Yon.emitter.emit('style', `@scope (.ty-${hash}) { ${style.innerHTML} }`)

        const outers: string[] = []
        const inners: string[] = []

        html.forEach(h => {
            if(h.static) outers.push(h.static)
            if(h.element) {
                const temp = new JSDOM('').window.document.createElement('div');
                temp.innerHTML = h.element
                temp.children[0].classList.add(`ty-${hash}`)
                inners.push(`elements += ${temp.innerHTML}`)
            }
            if(h.render) inners.push(h.render)
        })

        return `
            
            export default async function(props) {

                ${scriptTag ? scriptTag.innerHTML : ''}

                ${outers.join('\n')}

                props?.split(';').map(exp => eval(exp))

                return function(execute) {

                    if(execute) {
                        const { classId, compId, func } = execute
                        if(classId === "ty-${hash}" || compId === "ty-${hash}") {
                            eval(func)
                        }
                    }

                    let elements = '';
                
                    ${inners.join('\n')}
                    
                    return elements
                }
            }
        `
    }

    private static async addToStatix(html: HTMLDivElement, script: HTMLScriptElement, style: HTMLStyleElement, route: string, dir: 'pages' | 'components') {

        const module = Yon.parseHTML(html.children)

        const jsData = Yon.createJSData(module, script, style)

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

                Router.reqRoutes[`/assets/${route}`] = {
                    GET: async () => new Response(await Bun.file(`${Router.assetsPath}/${route}`).text())
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

                const { html, script, style } = Yon.extractComponents(data)
                
                await Yon.addToStatix(html, script, style, `${route}.${script?.lang || 'js'}`, 'pages')
            }
        }

        const nfFile = Bun.file(`${process.cwd()}/404.html`)

        const data = await nfFile.exists() ? await nfFile.text() : await Bun.file(`${import.meta.dir}/404.html`).text()

        const { html, script, style } = Yon.extractComponents(data)
        
        await Yon.addToStatix(html, script, style, '404.html', 'pages')
    }

    private static async bundleComponents() {

        if(await exists(Router.componentsPath)) {

            const components = Array.from(new Bun.Glob(`**/*.html`).scanSync({ cwd: Router.componentsPath }))

            for(let comp of components) {

                const folders = comp.split('/')

                const filename = folders[folders.length - 1].replace('.html', '')

                Yon.compMapping.set(filename, comp.replace('.html', '.js'))

                const data = await Bun.file(`${Router.componentsPath}/${comp}`).text()

                const { html, script, style } = Yon.extractComponents(data)
                
                await Yon.addToStatix(html, script, style, comp, 'components')
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