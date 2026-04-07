import { BunRequest, Server } from "bun"

/** Context object injected into every route handler */
export interface RequestContext {
    ipAddress: string
    bearer?: {
        header: Record<string, unknown>
        payload: Record<string, unknown>
        signature: string
        /** Always false — signature is decoded but NOT cryptographically verified */
        verified: false
    }
}

/** Standard route response envelope */
export interface RouteResponse {
    status: number
    body?: string
}

/** Parsed inbound request payload forwarded to route handlers via stdin */
export interface RequestPayload {
    headers?: Record<string, string>
    paths?: Record<string, string>
    body?: unknown
    query?: Record<string, unknown>
}

/** Per-route validation schema loaded from an OPTIONS file */
export interface RouteOptions {
    request?: RequestPayload
    response?: Record<number, unknown>
}

/** Middleware hooks that intercept every request and response */
export interface Middleware {
    before?: (request: Request, context: RequestContext) => Promise<Response | void> | Response | void
    after?:  (request: Request, response: Response, context: RequestContext) => Promise<Response> | Response
}

export default class Router {

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    static readonly reqRoutes: Record<string, Record<string, (req?: BunRequest, server?: Server<any>) => Promise<Response> | Response>> = {}

    static readonly allRoutes = new Map<string, Set<string>>()

    static readonly routeSlugs: Record<string, Record<string, number>> = {}

    static readonly routesPath     = process.env.ROUTES_PATH     || `${process.cwd()}/routes`
    static readonly componentsPath = process.env.COMPONENTS_PATH || `${process.cwd()}/components`
    static readonly assetsPath     = process.env.ASSETS_PATH     || `${process.cwd()}/assets`
    static readonly middlewarePath = process.env.MIDDLEWARE_PATH  || `${process.cwd()}/middleware`

    static middleware: Middleware | null = null

    static readonly routeConfigs: Record<string, Record<string, RouteOptions>> = {}

    static resetStaticState() {
        for (const key of Object.keys(Router.reqRoutes)) delete Router.reqRoutes[key]
        Router.allRoutes.clear()
        for (const key of Object.keys(Router.routeSlugs)) delete Router.routeSlugs[key]
        for (const key of Object.keys(Router.routeConfigs)) delete Router.routeConfigs[key]
    }

    private static readonly allMethods = process.env.ALLOW_METHODS
        ? process.env.ALLOW_METHODS.split(',')
        : ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD']

    static headers = {
        "Access-Control-Allow-Headers":    process.env.ALLOW_HEADERS     || "",
        "Access-Control-Allow-Origin":     process.env.ALLOW_ORIGINS     || "",
        "Access-Control-Allow-Credential": process.env.ALLOW_CREDENTIALS || "false",
        "Access-Control-Expose-Headers":   process.env.ALLOW_EXPOSE_HEADERS || "",
        "Access-Control-Max-Age":          process.env.ALLOW_MAX_AGE     || "",
        "Access-Control-Allow-Methods":    process.env.ALLOW_METHODS     || "",
        // Security headers
        "X-Frame-Options":                 "DENY",
        "X-Content-Type-Options":          "nosniff",
        "Strict-Transport-Security":       "max-age=31536000; includeSubDomains",
        "Content-Security-Policy":         process.env.CONTENT_SECURITY_POLICY || "default-src 'self'",
        "Referrer-Policy":                 "strict-origin-when-cross-origin",
    }

    /**
     * Validates a route file path, registers slugs, and records allowed methods.
     * Throws if the route is malformed or a duplicate.
     * @param route - Relative path from the routes directory (e.g. `api/:id/GET`)
     * @param staticPaths - Accumulator used to detect duplicate static segments
     */
    static async validateRoute(route: string, staticPaths: string[] = []) {

        const paths = route.split('/')

        const slugPattern = /^:.*/

        const slugs: Record<string, number> = {}

        if (slugPattern.test(paths[0])) throw new Error(`Invalid route: '${route}' — route cannot start with a slug segment`)

        paths.forEach((path, idx) => {

            if (slugPattern.test(path) && (slugPattern.test(paths[idx - 1]) || slugPattern.test(paths[idx + 1]))) {
                throw new Error(`Invalid route: '${route}' — consecutive slug segments are not allowed`)
            }

            if (slugPattern.test(path)) slugs[path] = idx
        })

        const staticPath = paths.filter((path) => !slugPattern.test(path)).join(',')

        if (staticPaths.includes(staticPath)) throw new Error(`Duplicate route: '${route}'`)

        staticPaths.push(staticPath)

        const method = paths.pop()!

        route = `/${paths.join('/')}`

        const optionsFile = Bun.file(`${Router.routesPath}${route}/OPTIONS`)

        if (!Router.allRoutes.has(route)) Router.allRoutes.set(route, new Set<string>())

        Router.allRoutes.get(route)?.add(method)

        if (await optionsFile.exists() && !Router.routeConfigs[route]) {
            Router.routeConfigs[route] = await optionsFile.json()
            Router.allRoutes.get(route)?.add('OPTIONS')
        }

        if (Object.keys(slugs).length > 0 || method === 'HTML') Router.routeSlugs[route] = slugs
    }

    /**
     * Extracts headers, body, and query parameters from a request and resolves
     * the filesystem handler path for the matched route.
     * @param request - The incoming Bun request
     * @param route - The matched route pattern
     * @returns Handler path, parsed stdin payload, and optional route config
     */
    static async processRequest(request: BunRequest, route: string) {

        const stdin: RequestPayload = {
            paths: request.params
        }

        let requestConfig: RouteOptions | undefined

        if (Router.routeConfigs[route]?.[request.method]) {
            requestConfig = Router.routeConfigs[route][request.method]
        }

        stdin.headers = request.headers.toJSON()

        const blob = await request.blob()

        if (blob.size > 0) {
            const contentType = request.headers.get('content-type') ?? ''
            stdin.body = contentType.includes('json') ? await blob.json() : await blob.text()
        }

        const searchParams = new URL(request.url).searchParams

        if (searchParams.size > 0) {
            stdin.query = Router.parseKVParams(searchParams)
        }

        return { handler: `${Router.routesPath}${route}/${request.method}`, stdin, config: requestConfig }
    }

    /** Maximum allowed length for any single route or query parameter value. */
    static readonly MAX_PARAM_LENGTH = Number(process.env.MAX_PARAM_LENGTH) || 1000

    /**
     * Coerces an array of string path segments into their native types
     * (number, boolean, null, undefined, or string).
     * @param input - Raw string segments from the URL path
     * @returns Typed parameter values
     * @throws Response with status 400 if any segment exceeds MAX_PARAM_LENGTH
     */
    static parseParams(input: string[]): (string | boolean | number | null | undefined)[] {

        const params: (string | boolean | number | null | undefined)[] = []

        for (const param of input) {
            if (param.length > Router.MAX_PARAM_LENGTH) {
                throw Response.json({ error: 'Parameter too long' }, { status: 400 })
            }

            const num = Number(param)

            if (!Number.isNaN(num))         params.push(num)
            else if (param === 'true')      params.push(true)
            else if (param === 'false')     params.push(false)
            else if (param === 'null')      params.push(null)
            else if (param === 'undefined') params.push(undefined)
            else                            params.push(param)
        }

        return params
    }

    /**
     * Parses key-value pairs from {@link URLSearchParams} or {@link FormData},
     * coercing each value to its native type where possible.
     * @param input - The source key-value collection
     * @returns A record of coerced parameter values
     */
    private static parseKVParams(input: URLSearchParams | FormData): Record<string, unknown> {

        const params: Record<string, unknown> = {}

        for (const [key, val] of input) {

            if (typeof val === "string" && val.length > Router.MAX_PARAM_LENGTH) {
                throw Response.json({ error: 'Parameter too long' }, { status: 400 })
            }

            if (typeof val === "string") {

                try {
                    const parsed = JSON.parse(val)
                    // Only accept primitive types and plain arrays/objects — reject prototypes
                    if (parsed !== null && typeof parsed === "object" && Object.getPrototypeOf(parsed) !== Object.prototype && !Array.isArray(parsed)) {
                        params[key] = val
                    } else {
                        params[key] = parsed
                    }
                } catch {
                    const num = Number(val)

                    if (!Number.isNaN(num))    params[key] = num
                    else if (val === 'true')   params[key] = true
                    else if (val === 'false')  params[key] = false
                    else if (val.includes(',')) params[key] = Router.parseParams(val.split(','))
                    else if (val === 'null')   params[key] = null

                    if (params[key] === undefined) params[key] = val
                }

            } else params[key] = val
        }

        return params
    }

    /**
     * Scans the routes directory and validates every discovered route file.
     */
    static async validateRoutes() {

        const routes = Array.from(new Bun.Glob(`**/{${Router.allMethods.join(',')}}`).scanSync({ cwd: Router.routesPath }))

        for (const route of routes) await Router.validateRoute(route)
    }
}
