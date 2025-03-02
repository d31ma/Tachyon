import { $, BunRequest, Server } from 'bun'

export interface _ctx {
    request: Record<string, any>,
    slugs?: Record<string, any>,
    ipAddress: string,
    params?: Array<any>,
    query?: Record<string, any>,
    body: string
}

export default class Router {

    static readonly reqRoutes: Record<string, Record<string, (req: BunRequest, server: Server) => Promise<Response> | Response>> = {}

    static readonly allRoutes = new Map<string, Set<string>>()

    static readonly routeSlugs = new Map<string, Map<string, number>>()

    static readonly routesPath = `${process.cwd()}/routes`
    static readonly componentsPath = `${process.cwd()}/components`

    private static readonly allMethods = process.env.ALLOW_METHODS ? process.env.ALLOW_METHODS.split(',') : ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS']

    static headers = {
        "Access-Control-Allow-Headers": process.env.ALLOW_HEADERS || "",
        "Access-Control-Allow-Origin": process.env.ALLLOW_ORGINS || "",
        "Access-Control-Allow-Credential": process.env.ALLOW_CREDENTIALS || "false",
        "Access-Control-Expose-Headers": process.env.ALLOW_EXPOSE_HEADERS || "",
        "Access-Control-Max-Age": process.env.ALLOW_MAX_AGE || "",
        "Access-Control-Allow-Methods": process.env.ALLOW_METHODS || ""
    }

    static async validateRoute(route: string, staticPaths: string[] = []) {

        const paths = route.split('/')
    
        const pattern = /^:.*/

        const slugs = new Map<string, number>()

        if(pattern.test(paths[0])) throw new Error(`Invalid route ${route}`)

        paths.forEach((path, idx) => {

            if(pattern.test(path) && (pattern.test(paths[idx - 1]) || pattern.test(paths[idx + 1]))) {
                throw new Error(`Invalid route ${route}`)
            }

            if(pattern.test(path)) slugs.set(path, idx)
        })

        const staticPath = paths.filter((path) => !pattern.test(path)).join(',')

        if(staticPaths.includes(staticPath)) throw new Error(`Duplicate route ${route}`)

        staticPaths.push(staticPath)

        await $`chmod +x ${Router.routesPath}/${route}`

        const method = paths.pop()!

        route = `/${paths.join('/')}`

        if(!Router.allRoutes.has(route)) Router.allRoutes.set(route, new Set<string>())

        Router.allRoutes.get(route)?.add(method)

        if(slugs.size > 0 || method === 'HTML') Router.routeSlugs.set(route, slugs)
    }

    static parseRequest(request: BunRequest) {

        const req: Record<string, any> = {}

        req.headers = Object.fromEntries(request.headers)
        req.cache = request.cache
        req.credentials = request.credentials
        req.destination = request.destination
        req.integrity = request.integrity
        req.keepalive = request.keepalive
        req.method = request.method
        req.mode = request.mode
        req.redirect = request.redirect
        req.referrer = request.referrer
        req.referrerPolicy = request.referrerPolicy
        req.url = request.url
        req.parans = request.params

        return req
    }

    static processRequest(request: BunRequest, route: string, ctx: _ctx) {
    
        const { params } = Router.getParams(request, route)

        ctx.slugs = request.params

        const searchParams = new URL(request.url).searchParams

        let queryParams: Record<string, any> | undefined;

        if(searchParams.size > 0) queryParams = Router.parseKVParams(searchParams)

        ctx.params = params
        ctx.query = queryParams

        return { handler: `${Router.routesPath}${route}/${request.method}`, ctx }
    }

    private static getParams(request: BunRequest, route: string) {

        const url = new URL(request.url)

        const params = url.pathname.split("/").slice(route.split("/").length)

        return { params: Router.parseParams(params) }
    }

    static parseParams(input: string[]) {

        const params: (string | boolean | number | null | undefined)[] = []

        for(const param of input) {

            const num = Number(param)

            if(!Number.isNaN(num)) params.push(num)

            else if(param === 'true') params.push(true)

            else if(param === 'false') params.push(false)

            else if(param === 'null') params.push(null)

            else if(param === 'undefined') params.push(undefined)

            else params.push(param)
        }

        return params
    }

    private static parseKVParams(input: URLSearchParams | FormData) {

        const params: Record<string, any> = {}

        for(const [key, val] of input) {

            if(typeof val === "string") {

                try {

                    params[key] = JSON.parse(val)
    
                } catch {
    
                    const num = Number(val)
    
                    if(!Number.isNaN(num)) params[key] = num
    
                    else if(val === 'true') params[key] = true
    
                    else if(val === 'false') params[key] = false
    
                    else if(typeof val === "string" && val.includes(',')) params[key] = Router.parseParams(val.split(','))
    
                    else if(val === 'null') params[key] = null
    
                    if(params[key] === undefined) params[key] = val
                }

            } else params[key] = val
        }

        return params
    }
    
    static async validateRoutes() {

        const routes = Array.from(new Bun.Glob(`**/{${Router.allMethods.join(',')},SOCKET}`).scanSync({ cwd: Router.routesPath }))
        
        for(const route of routes) await Router.validateRoute(route)
    }
}