import { AsyncLocalStorage } from "node:async_hooks";
import { watch } from "node:fs";
import Silo from "@vyckr/byos";
import { Glob, Server } from "bun";

const Tach = {

    indexedRoutes: new Map<string, Map<string, Function>>(),

    routeSlugs: new Map<string, Map<string, number>>(),

    allMethods: process.env.ALLOW_METHODS ? process.env.ALLOW_METHODS.split(',') : ['GET', 'POST', 'PUT', 'DELETE', 'PATCH'],

    inDevelopment: process.env.PRODUCTION ? false : true,

    headers: {
        "Access-Control-Allow-Headers": process.env.ALLOW_HEADERS || "",
        "Access-Control-Allow-Origin": process.env.ALLLOW_ORGINS || "",
        "Access-Control-Allow-Credential": process.env.ALLOW_CREDENTIALS || "false",
        "Access-Control-Expose-Headers": process.env.ALLOW_EXPOSE_HEADERS || "",
        "Access-Control-Max-Age": process.env.ALLOW_MAX_AGE || "",
        "Access-Control-Allow-Methods": process.env.ALLOW_METHODS || ""
    },

    dbPath: process.env.DB_DIR,

    saveStats: process.env.SAVE_STATS === 'true',
    saveRequests: process.env.SAVE_REQUESTS === 'true',
    saveErrors: process.env.SAVE_ERRORS === 'true',
    saveLogs: process.env.SAVE_LOGS === 'true',

    logsTableName: "_logs",
    errorsTableName: "_errors",
    requestTableName: "_requests",
    statsTableName: "_stats",

    context: new AsyncLocalStorage<_log[]>(),

    routesPath: process.env.LAMBDA_TASK_ROOT ? `${process.env.LAMBDA_TASK_ROOT}/routes` : `${process.cwd()}/routes`,

    proxyMod: null,

    async proxy(req: Request, server?: Server) {

        const request = req.clone()

        const logs: _log[] = []

        const url = new URL(req.url)
        
        const startTime = Date.now()

        const ipAddress = server && server.requestIP ? server.requestIP(req)!.address : '0.0.0.0'

        return await Tach.context.run(logs, async () => {

            let res: Response

            try {

                const data = await Tach.processRequest(req, { ipAddress, request: req, requestTime: startTime, logs, slugs: new Map<string, any>() })
    
                res = Tach.processResponse(200, data)

                if(logs.length > 0 && Tach.saveLogs && Tach.dbPath) await Promise.all(logs.map(log => { 
                                        return Silo.putData(Tach.logsTableName, { ipAddress, path: url.pathname, method: req.method, ...log })
                                    }))
            
                if(!Tach.isAsyncIterator(data)) {

                    const status = res.status
                    const response_size = typeof data !== "undefined" ? String(data).length : 0
                    const url = new URL(req.url)
                    const method = req.method
                    const date = Date.now()
                    const duration = date - startTime
                    
                    console.info(`"${method} ${url.pathname}" ${status} - ${duration}ms - ${response_size} byte(s)`)
                
                    if(Tach.dbPath && Tach.saveStats) await Silo.putData(Tach.statsTableName, { ipAddress, cpu: process.cpuUsage(), memory: process.memoryUsage(), date: Date.now() })
                }

            } catch(e) {

                const method = request.method

                await Tach.logError(e as Error, ipAddress, url, method, logs, startTime)

                if(Tach.dbPath && Tach.saveStats) await Silo.putData(Tach.statsTableName, { ipAddress, cpu: process.cpuUsage(), memory: process.memoryUsage(), date: Date.now() })

                res = Response.json({ detail: (e as Error).message }, { status: (e as Error).cause as number ?? 500, headers: Tach.headers })
            }
            
            return res
        })
    },
    
    pathsMatch(routeSegs: string[], pathSegs: string[]) {

        if (routeSegs.length !== pathSegs.length) {
            return false;
        }

        const slugs = Tach.routeSlugs.get(`${routeSegs.join('/')}/index.ts`) || Tach.routeSlugs.get(`${routeSegs.join('/')}/index.js`) || new Map<string, number>()
    
        for (let i = 0; i < routeSegs.length; i++) {
            if (!slugs.has(routeSegs[i]) && routeSegs[i] !== pathSegs[i]) {
                return false;
            }
        }
    
        return true;
    },

    getHandler(request: Request) {

        const url = new URL(request.url);

        let handler;
        let params: string[] = [];
        const paths = url.pathname.split('/').slice(1);
        const allowedMethods: string[] = [];

        let slugs = new Map<string, string>()

        let bestMatchKey = '';
        let bestMatchLength = -1;

        for (const [routeKey] of Tach.indexedRoutes) {

            const routeSegs = routeKey.split('/')

            routeSegs.pop()
            
            const isMatch = Tach.pathsMatch(routeSegs, paths.slice(0, routeSegs.length));

            if (isMatch && routeSegs.length > bestMatchLength) {
                bestMatchKey = routeKey;
                bestMatchLength = routeSegs.length;
            }
        }

        if (bestMatchKey) {
            const routeMap = Tach.indexedRoutes.get(bestMatchKey)!
            handler = routeMap.get(request.method);

            for (const [key] of routeMap) {
                if (Tach.allMethods.includes(key)) allowedMethods.push(key);
            }

            params = paths.slice(bestMatchLength);

            const slugMap = Tach.routeSlugs.get(bestMatchKey) ?? new Map<string, number>()

            slugMap.forEach((idx, key) => slugs.set(key, paths[idx]))
        }

        Tach.headers = { ...Tach.headers, "Access-Control-Allow-Methods": allowedMethods.join(',') };

        if (!handler) throw new Error(`Route ${request.method} ${url.pathname} not found`, { cause: 404 });

        return { handler, params: Tach.parseParams(params), slugs }
    },

    formatDate() {
        return new Date().toISOString().replace('T', ' ').replace('Z', '')
    },

    formatMsg(...msg: any[]) {

        const formatted: string[] = []

        for(const arg of msg) {

            if(arg instanceof Set) formatted.push(JSON.stringify(Array.from(arg), null, 2))

            else if(arg instanceof Map) formatted.push(JSON.stringify(Object.fromEntries(arg), null, 2))

            else if(arg instanceof FormData) {
                const formEntries: Record<string, any> = {}
                arg.forEach((val, key) => formEntries[key] = val)
                formatted.push(JSON.stringify(formEntries, null, 2))
            }

            else if(Array.isArray(arg) || (typeof arg === 'object' && !Array.isArray(arg)) || (typeof arg === 'object' && arg !== null)) formatted.push(JSON.stringify(arg, null, 2))

            else formatted.push(arg)
        }

        return formatted.join('\n\n')
    },

    configLogger() {

        const reset = '\x1b[0m'

        console.info = (...args: any[]) => {
            const info = `[${Tach.formatDate()}]\x1b[32m INFO${reset} (${process.pid}) ${Tach.formatMsg(...args)}`
            console.log(info)
            if(Tach.context.getStore()) {
                const logWriter = Tach.context.getStore()
                if(logWriter && Tach.dbPath && Tach.saveLogs) logWriter.push({ date: Date.now(), msg: `${info.replace(reset, '').replace('\x1b[32m', '')}\n`, type: "info" })
            }
        }

        console.error = (...args: any[]) => {
            const err = `[${Tach.formatDate()}]\x1b[31m ERROR${reset} (${process.pid}) ${Tach.formatMsg(...args)}`
            console.log(err)
            if(Tach.context.getStore()) {
                const logWriter = Tach.context.getStore()
                if(logWriter && Tach.dbPath && Tach.saveLogs) logWriter.push({ date: Date.now(), msg: `${err.replace(reset, '').replace('\x1b[31m', '')}\n`, type: "error" })
            }
        }

        console.debug = (...args: any[]) => {
            const bug = `[${Tach.formatDate()}]\x1b[36m DEBUG${reset} (${process.pid}) ${Tach.formatMsg(...args)}`
            console.log(bug)
            if(Tach.context.getStore()) {
                const logWriter = Tach.context.getStore()
                if(logWriter && Tach.dbPath && Tach.saveLogs) logWriter.push({ date: Date.now(), msg: `${bug.replace(reset, '').replace('\x1b[36m', '')}\n`, type: "debug" })
            }
        }

        console.warn = (...args: any[]) => {
            const warn = `[${Tach.formatDate()}]\x1b[33m WARN${reset} (${process.pid}) ${Tach.formatMsg(...args)}`
            console.log(warn)
            if(Tach.context.getStore()) {
                const logWriter = Tach.context.getStore()
                if(logWriter && Tach.dbPath && Tach.saveLogs) logWriter.push({ date: Date.now(), msg: `${warn.replace(reset, '').replace('\x1b[33m', '')}\n`, type: "warn" })
            }
        }

        console.trace = (...args: any[]) => {
            const trace = `[${Tach.formatDate()}]\x1b[35m TRACE${reset} (${process.pid}) ${Tach.formatMsg(...args)}`
            console.log(trace)
            if(Tach.context.getStore()) {
                const logWriter = Tach.context.getStore()
                if(logWriter && Tach.dbPath && Tach.saveLogs) logWriter.push({ date: Date.now(), msg: `${trace.replace(reset, '').replace('\x1b[35m', '')}\n`, type: "trace" })
            }
        }
    },

    async logRequest(request: Request, status: number, context: _HTTPContext, data: any = null) {

        if(Tach.dbPath && Tach.saveRequests) {

            const url = new URL(request.url)
            const date = Date.now()
            const duration = date - (context.requestTime ?? 0)

            await Silo.putData(Tach.requestTableName, { ipAddress: context.ipAddress, url: `${url.pathname}${url.search}`, method: request.method, status, duration, date, size: data ? String(data).length : 0, data })
        }
    },

    async processRequest(request: Request, context: _HTTPContext) {

        const { handler, params, slugs } = Tach.getHandler(request)

        if(slugs.size > 0) context.slugs = slugs

        const body = await request.blob()

        let data: Blob | Record<string, any> | undefined

        if(body.size > 0) {

            if(body.type.includes('form')) data = Tach.parseKVParams(await body.formData())
            else {
                try {
                    data = await body.json()
                } catch {
                    data = body
                }
            }
        }

        const searchParams = new URL(request.url).searchParams

        let queryParams: Record<string, any> | undefined;

        if(searchParams.size > 0) queryParams = Tach.parseKVParams(searchParams)

        if(params.length > 0 && !queryParams && !data) {

            const res = await handler(...params, context)

            await Tach.logRequest(request, 200, context)

            return res

        } else if(params.length === 0 && queryParams && !data) {

            const res = await handler(queryParams, context)

            await Tach.logRequest(request, 200, context)

            return res

        } else if(params.length === 0 && !queryParams && data) {

            const res = await handler(data, context)

            await Tach.logRequest(request, 200, context, await body.text())

            return res

        } else if(params.length > 0 && queryParams && !data) {

            const res = await handler(...params, queryParams, context)

            await Tach.logRequest(request, 200, context)

            return res
        
        } else if(params.length > 0 && !queryParams && data) {

            const res = await handler(...params, data, context)

            await Tach.logRequest(request, 200, context, await body.text())

            return res

        } else if(params.length === 0 && data && queryParams) {

            const res = await handler(queryParams, data, context)

            await Tach.logRequest(request, 200, context, await body.text())

            return res
        
        } else if(params.length > 0 && data && queryParams) {

            const res = await handler(...params, queryParams, data, context)

            await Tach.logRequest(request, 200, context, await body.text())

            return res
        
        } else {

            const res = await handler(context)

            await Tach.logRequest(request, 200, context)

            return res
        }
    },

    isAsyncIterator(data: any) {
        return typeof data === "object" && Object.hasOwn(data, Symbol.asyncIterator)
    },

    hasFunctions(data: any) {
        return typeof data === "object" && (Object.keys(data).some((elem) => typeof elem === "function") || Object.values(data).some((elem) => typeof elem === "function"))
    },

    processResponse(status: number, data?: any) {

        const headers = Tach.headers

        if(data instanceof Set) return Response.json(Array.from(data), { status, headers }) 
        
        if(data instanceof Map) return Response.json(Object.fromEntries(data), { status, headers })

        if(data instanceof FormData || data instanceof Blob) return new Response(data, { status, headers })

        if(typeof data === "object" && !Array.isArray(data) && !Tach.isAsyncIterator(data) && !Tach.hasFunctions(data)) return Response.json(data, { status, headers })

        if((typeof data === "object" && Array.isArray(data)) || data instanceof Array) return Response.json(data, { status, headers })

        if(typeof data === "number" || typeof data === "boolean") return Response.json(data, { status, headers })
    
        return new Response(data, { status, headers })
    },

    async logError(e: Error, ipAddress: string, url: URL, method: string, logs: _log[], startTime?: number) {

        const path = url.pathname

        if(logs.length > 0 && Tach.saveLogs && Tach.dbPath) await Promise.all(logs.map(log => {
                                return Silo.putData(Tach.logsTableName, { ipAddress, path, method, ...log })
                            }))

        if(Tach.dbPath && Tach.saveErrors) await Silo.putData(Tach.errorsTableName, { ipAddress, date: Date.now(),path, method, error: e.message })

        console.error(`"${method} ${path}" ${e.cause as number ?? 500} ${startTime ? `- ${Date.now() - startTime}ms` : ''} - ${e.message.length} byte(s)`)
    },

    watchFiles() {
        
        if(Tach.inDevelopment) {

            watch(Tach.routesPath, { recursive: true }, async (ev, filename) => {
                delete import.meta.require.cache[`${Tach.routesPath}/${filename}`]
                if(!filename?.split('/').some((path) => path.startsWith('_'))) await Tach.validateRoutes(filename!)
            })
        }
    },

    async fetch(req: Request, server: Server) {

        if(Tach.proxyMod) {

            const middleware = (Tach.proxyMod as any).default

            if(middleware) return await middleware(req, Tach.proxy)
        }

        return await Tach.proxy(req, server)
    },

    async validateRoutes(route?: string) {

        const staticPaths: string[] = []

        const validateRoute = async (route: string) => {  

            const paths = route.split('/')
    
            const pattern = /[<>|\[\]]/

            const slugs = new Map<string, number>()

            if(pattern.test(paths[0]) || pattern.test(paths[paths.length - 1])) throw new Error(`Invalid route ${route}`)

            paths.forEach((path, idx) => {

                if(pattern.test(path) && (pattern.test(paths[idx - 1]) || pattern.test(paths[idx + 1]))) {
                    throw new Error(`Invalid route ${route}`)
                }

                if(pattern.test(path)) slugs.set(path, idx)
            })
    
            const staticPath = paths.filter((path) => !pattern.test(path)).join(',')
    
            if(staticPaths.includes(staticPath)) throw new Error(`Duplicate route ${route}`)
    
            staticPaths.push(staticPath)

            const module = await import(`${Tach.routesPath}/${route}`)

            const controller = (new module.default() as any).constructor

            const methodFuncs = new Map<string, Function>()

            for(const method of Tach.allMethods) {

                if(controller[method]) {

                    methodFuncs.set(method, controller[method])
                }
            }

            Tach.indexedRoutes.set(route, methodFuncs)

            if(slugs.size > 0) Tach.routeSlugs.set(route, slugs)
        }

        if(route) return await validateRoute(route)

        const routes = Array.from(new Glob(`**/*/index.{ts,js}`).scanSync({ cwd: Tach.routesPath }))
        
        for(const route of routes) await validateRoute(route)

        const proxy = Array.from(new Glob(`**/proxy.{ts,js}`).scanSync({ cwd: Tach.routesPath }))

        if(proxy[0]) Tach.proxyMod = await import(`${Tach.routesPath}/${proxy[0]}`)
    },

    parseParams(input: string[]) {

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
    },

    parseKVParams(input: URLSearchParams | FormData) {

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
}

try {

    await Tach.validateRoutes()

    Tach.watchFiles()
    
    Tach.configLogger()

} catch(e) {
    console.log(`Tach.ts --> ${e}`)
}

export default Tach