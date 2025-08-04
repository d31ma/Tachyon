import { Subprocess, ServerWebSocket, BunFile, BunRequest, Server } from "bun";
import Router, { _ctx } from "../router.js";
import { rm } from "fs/promises";

export default class Tach {

    static webSockets = new Map<ServerWebSocket<any>, Subprocess<"pipe", "inherit", "pipe">>()

    static headers = {
        "Access-Control-Allow-Headers": process.env.ALLOW_HEADERS || "",
        "Access-Control-Allow-Origin": process.env.ALLLOW_ORGINS || "",
        "Access-Control-Allow-Credential": process.env.ALLOW_CREDENTIALS || "false",
        "Access-Control-Expose-Headers": process.env.ALLOW_EXPOSE_HEADERS || "",
        "Access-Control-Max-Age": process.env.ALLOW_MAX_AGE || "",
        "Access-Control-Allow-Methods": process.env.ALLOW_METHODS || ""
    }

    static parseRequest(request: Request) {

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

        return req
    }

    private static getParams(request: BunRequest, route: string) {
    
        const url = new URL(request.url)

        const params = url.pathname.slice(route.length).split('/')

        return { params: Tach.parseParams(params) }
    }

    private static parseParams(input: string[]) {

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
    
                    else if(typeof val === "string" && val.includes(',')) params[key] = Tach.parseParams(val.split(','))
    
                    else if(val === 'null') params[key] = null
    
                    if(params[key] === undefined) params[key] = val
                }

            } else params[key] = val
        }

        return params
    }

    static processRequest(request: BunRequest, route: string, ctx: _ctx) {

        const { params } = Tach.getParams(request, route)

        ctx.slugs = request.params

        const searchParams = new URL(request.url).searchParams

        let queryParams: Record<string, any> | undefined;

        if(searchParams.size > 0) queryParams = Tach.parseKVParams(searchParams)

        ctx.params = params
        ctx.query = queryParams

        return { ctx }
    }

    private static async processResponse(cmd: string[], input: _ctx) {

        const proc = Bun.spawn({
            cmd,
            stdout: 'inherit',
            stderr: "pipe",
            stdin: "pipe"
        })

        proc.stdin.write(JSON.stringify(input))
        proc.stdin.end()

        let exitCode = await proc.exited

        const stderrContent = await new Response(proc.stderr).text();
        if(exitCode !== 0 && stderrContent.length > 0) {
            exitCode = exitCode < 0 || (exitCode > 0 && exitCode < 100) || exitCode > 599 ? 500 : exitCode
            return { status: exitCode, body: proc.stderr.toString() }
        }

        exitCode = exitCode === 0 ? 200 : exitCode

        return { status: exitCode, body: Bun.file(`/tmp/${proc.pid}`) }
    }

    private static async serveRequest(handler: string, ctx: _ctx) {

        const res = await Tach.processResponse([handler], ctx) as { status: number, body?: BunFile }
            
        const size = res.body ? res.body.size : 0
        
        const response = new Response(res.body, { status: res.status, headers: Tach.headers })

        return { response, size }
    }

    static createServerRoutes() {

        if(!Router.allRoutes.has('/')) Router.allRoutes.set('/', new Set(['GET']))

        for(const [route, methods] of Router.allRoutes) {

            const serverRoute = async (request?: BunRequest, server?: Server) => {

                const start = Date.now()
        
                let res: Response
                let bodySize: number
                const path = new URL(request!.url).pathname
                const body = `/tmp/${Bun.randomUUIDv7()}`

                try {

                    const accept = request!.headers.get('accept') || ''

                    if(accept.includes('text/html')) {
                        return new Response(await Bun.file(`${import.meta.dir}/../client/dev.html`).text(), { status: 200, headers: { 'Content-Type': 'text/html' } })  
                    }

                    request!.blob().then(async blob => {
                        if(blob.size > 0) await Bun.write(body, blob)
                    })

                    const { handler, ctx } = Router.processRequest(request!, route,  { 
                        request: Router.parseRequest(request!),
                        ipAddress: server?.requestIP(request!) ? server.requestIP(request!)!.address : '0.0.0.0',
                        body
                    })

                    const { response, size } = await Tach.serveRequest(handler, ctx!)

                    res = response
                    bodySize = size

                } catch(err) {

                    const error = err as Error

                    bodySize = error.message.length

                    res = Response.json({ error: error.message }, { status: error.cause as number, headers: Tach.headers })
                
                } finally {

                    Bun.file(body).exists().then(async exists => {
                        if(exists) await rm(body, { recursive: true })
                    })
                }

                const status = res.status
                const method = request!.method
                const duration = Date.now() - start

                console.info(`${path} - ${method} - ${status} - ${duration}ms - ${bodySize} byte(s)`)

                return res
            }

            for(const method of methods) {

                if(method !== 'HTML' && method !== 'SOCKET') {

                    if(!Router.reqRoutes[route] || !Router.reqRoutes[`${route}/*`]) {
                        Router.reqRoutes[route] = {}
                        Router.reqRoutes[`${route}/*`] = {}
                    }

                    Router.reqRoutes[route][method] = serverRoute
                    Router.reqRoutes[`${route}/*`][method] = serverRoute
                }
            }

            if(Router.reqRoutes['//*']) delete Router.reqRoutes['//*']
        }
    }
}