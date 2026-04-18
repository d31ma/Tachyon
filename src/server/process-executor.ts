import { BunRequest, Server } from "bun";
import { randomUUID, timingSafeEqual } from "node:crypto";
import Router, { RequestContext, RouteOptions, RequestPayload, RouteResponse } from "./route-handler.js";
import Pool from "./process-pool.js";
import Validate from "./schema-validator.js";
import logger from './logger.js'

export default class Tach {
    private static readonly processLogger = logger.child({ scope: 'process-executor' })
    private static readonly requestLogger = logger.child({ scope: 'http' })
    private static readonly handlerLogger = logger.child({ scope: 'handler' })

    private static readonly STREAM_MIME_TYPE = "text/event-stream"
    private static readonly REQUEST_ID_HEADER = 'X-Request-Id'
    private static readonly MAX_REQUEST_ID_LENGTH = 200
    private static readonly HTML_SHELL =
        process.env.NODE_ENV === 'production' ? 'production.html' : 'development.html'
    private static frontendRequestHandler:
        | ((request: BunRequest) => Promise<Response | null>)
        | null = null

    /**
     * Shared decoder instance — safe to reuse since we call decode() without
     * the `{stream: true}` option, making each call stateless.
     */
    private static readonly decoder = new TextDecoder()

    /**
     * Drains a readable byte stream line-by-line, calling `onLine` for each
     * complete newline-terminated line, and returns any trailing partial line.
     *
     * Reading stdout and stderr through this helper concurrently (via
     * `Promise.all`) prevents the classic pipe-buffer deadlock where a
     * subprocess blocks writing to stderr while the parent is still draining
     * stdout.
     *
     * @param stream - The byte stream to consume
     * @param onLine - Called for each complete line (excluding the newline)
     * @returns The final partial line (the response body fragment)
     */
    private static async drainStream(
        stream: ReadableStream<Uint8Array>,
        onLine: (line: string) => void,
    ): Promise<string> {
        let partial = ""

        for await (const chunk of stream) {
            const combined = partial + Tach.decoder.decode(chunk)
            const lines    = combined.split('\n')
            partial        = lines.pop()!

            for (const line of lines) {
                if (line.length > 0) onLine(line)
            }
        }

        return partial
    }

    private static byteLength(value: string | undefined): number {
        return value ? Buffer.byteLength(value) : 0
    }

    private static handlerErrorBody(stderr?: string): string {
        if (process.env.DEV === 'true' && process.env.DEV_ERROR_DETAILS === 'true' && stderr) {
            return JSON.stringify({ detail: stderr })
        }

        return JSON.stringify({ detail: 'Internal server error' })
    }

    private static async logHandlerResourceUsage(
        proc: ReturnType<typeof Pool.acquireHandler>,
        context: RequestContext,
        handler: string,
        responseBytes: number,
        errorBytes: number,
    ): Promise<void> {
        const exitCode = await proc.exited.catch(() => null)
        const usage = proc.resourceUsage()

        if (!usage) {
            Tach.handlerLogger.debug('Handler resource usage unavailable', {
                requestId: context.requestId,
                handler,
                pid: proc.pid,
                exitCode,
                responseBytes,
                errorBytes,
            })
            return
        }

        Tach.handlerLogger.info('Handler resource usage', {
            requestId: context.requestId,
            handler,
            pid: proc.pid,
            exitCode,
            cpuUserUs: usage.cpuTime.user,
            cpuSystemUs: usage.cpuTime.system,
            cpuTotalUs: usage.cpuTime.total,
            memoryMaxRssBytes: usage.maxRSS,
            fsReadOps: usage.ops.in,
            fsWriteOps: usage.ops.out,
            voluntaryContextSwitches: usage.contextSwitches.voluntary,
            involuntaryContextSwitches: usage.contextSwitches.involuntary,
            responseBytes,
            errorBytes,
        })
    }

    /**
     * Executes a handler and returns a single {@link RouteResponse}.
     * stdout and stderr are drained concurrently to prevent pipe-buffer deadlocks.
     */
    private static async getResponse(
        cmd:     string[],
        stdin:   RequestPayload,
        context: RequestContext,
        _config?: RouteOptions,
    ): Promise<RouteResponse> {

        const proc = Pool.acquireHandler(cmd[0])

        proc.stdin.write(JSON.stringify({ ...stdin, context }))
        proc.stdin.end()

        const [out, err] = await Promise.all([
            Tach.drainStream(proc.stdout, line => Tach.handlerLogger.info('Handler stdout output', {
                requestId: context.requestId,
                pid: proc.pid,
                output: line,
            })),
            Tach.drainStream(proc.stderr, line => Tach.handlerLogger.error('Handler stderr output', {
                requestId: context.requestId,
                pid: proc.pid,
                output: line,
            })),
        ])

        await Tach.logHandlerResourceUsage(
            proc,
            context,
            cmd[0],
            Tach.byteLength(out),
            Tach.byteLength(err),
        )

        if (out.length > 0) return { status: 200, body: out }
        if (err.length > 0) return { status: 500, body: Tach.handlerErrorBody(err) }
        return { status: 200 }
    }

    /**
     * Executes a handler and yields {@link RouteResponse} chunks as they arrive.
     * stderr is drained concurrently in the background so it never blocks stdout.
     */
    private static async *getStreamResponse(
        cmd:     string[],
        stdin:   RequestPayload,
        context: RequestContext,
        _config?: RouteOptions,
    ): AsyncGenerator<RouteResponse> {

        const proc = Pool.acquireHandler(cmd[0])

        proc.stdin.write(JSON.stringify({ ...stdin, context }))
        proc.stdin.end()

        // Drain stderr concurrently in the background so its OS buffer never fills
        // and blocks the subprocess while we are processing stdout chunks.
        const stderrLines: string[] = []
        let responseBytes = 0
        const stderrDone = Tach.drainStream(proc.stderr, line => {
            stderrLines.push(line)
            Tach.handlerLogger.error('Handler stderr output', {
                requestId: context.requestId,
                pid: proc.pid,
                output: line,
            })
        })

        for await (const chunk of proc.stdout) {
            const body = Tach.decoder.decode(chunk)
            const lines = body.split('\n')
            const partial = lines.pop()!

            for (const line of lines) {
                if (line.length > 0) {
                    Tach.handlerLogger.info('Handler stdout output', {
                        requestId: context.requestId,
                        pid: proc.pid,
                        output: line,
                    })
                }
            }

            responseBytes += Tach.byteLength(partial)
            if (partial.length > 0) yield { status: 200, body: partial }
        }

        const errRemainder = await stderrDone
        const errorBytes = Tach.byteLength(errRemainder)
            + stderrLines.reduce((total, line) => total + Tach.byteLength(line), 0)

        await Tach.logHandlerResourceUsage(
            proc,
            context,
            cmd[0],
            responseBytes,
            errorBytes,
        )

        if (errRemainder.length > 0) {
            yield { status: 500, body: Tach.handlerErrorBody(errRemainder) }
        } else if (stderrLines.length > 0) {
            yield { status: 500, body: Tach.handlerErrorBody(stderrLines.join(' ')) }
        }
    }

    private static isAuthorizedClient(authorization: string | undefined, basicAuth: string): boolean {
        if (!authorization) return false
        const [, provided] = authorization.split(' ')
        if (!provided) return false
        const expected = btoa(basicAuth)
        // Timing-safe comparison prevents brute-force timing oracle attacks
        const a = Buffer.from(expected)
        const b = Buffer.from(provided)
        if (a.length !== b.length) return false
        return timingSafeEqual(a, b)
    }

    static setFrontendRequestHandler(
        handler: ((request: BunRequest) => Promise<Response | null>) | null
    ) {
        Tach.frontendRequestHandler = handler
    }

    private static getRequestId(request: Request): string {
        const incoming = request.headers.get(Tach.REQUEST_ID_HEADER)?.trim()

        if (incoming && incoming.length <= Tach.MAX_REQUEST_ID_LENGTH) {
            return incoming
        }

        return randomUUID()
    }

    private static withRequestId(response: Response, requestId: string): Response {
        const headers = new Headers(response.headers)
        headers.set(Tach.REQUEST_ID_HEADER, requestId)

        return new Response(response.body, {
            status: response.status,
            statusText: response.statusText,
            headers,
        })
    }

    private static async serveRequest(
        handler:  string,
        stdin:    RequestPayload,
        context:  RequestContext,
        config?:  RouteOptions,
    ): Promise<Response> {

        if (process.env.BASIC_AUTH && !Tach.isAuthorizedClient(stdin.headers?.authorization, process.env.BASIC_AUTH)) {
            return Response.json(
                { detail: "Unauthorized Client" },
                { status: 401, headers: { ...Router.headers, "WWW-Authenticate": 'Basic realm="Secure Area"' } }
            )
        }

        if (process.env.VALIDATE !== undefined) {
            try {
                await Validate.validateData(handler, "req", stdin as Record<string, unknown>)
            } catch (e) {
                return Response.json({ detail: (e as Error).message }, { status: 400, headers: Router.headers })
            }
        }

        if (stdin.headers?.accept === Tach.STREAM_MIME_TYPE) {
            const stream = new ReadableStream({
                async start(controller) {
                    for await (const { body, status } of Tach.getStreamResponse([handler], stdin, context, config)) {
                        if (process.env.VALIDATE !== undefined) {
                            try {
                                await Validate.validateData(handler, status === 200 ? "res" : "err", body!)
                            } catch (e) {
                                controller.enqueue(JSON.stringify({ detail: (e as Error).message }))
                            }
                        } else {
                            controller.enqueue(body)
                        }
                    }
                    controller.close()
                }
            })

            return new Response(stream, { headers: { ...Router.headers, "Content-Type": Tach.STREAM_MIME_TYPE } })
        }

        const { body, status } = await Tach.getResponse([handler], stdin, context, config)

        const matchedStatus = body ? Validate.matchStatusCode(handler, body) : null
        const finalStatus   = matchedStatus ?? status

        if (process.env.VALIDATE !== undefined) {
            const ioKey = matchedStatus ? String(matchedStatus) : (status === 200 ? "res" : "err")
            try {
                await Validate.validateData(handler, ioKey, body!)
            } catch (e) {
                return Response.json({ detail: (e as Error).message }, { status: 422, headers: Router.headers })
            }
        }

        return new Response(body, { status: finalStatus, headers: Router.headers })
    }

    /**
     * Extracts a Bearer token without exposing decoded claims to handlers.
     * The token is decoded only internally to reject expired JWTs when possible.
     * @param authorization - The raw `Authorization` header value
     */
    private static getBearerContext(authorization: string | undefined): RequestContext['bearer'] {
        if (!authorization) return undefined

        const [authType, token] = authorization.split(' ')

        if (authType?.toLowerCase() !== "bearer" || !token) return undefined

        const [, payload] = token.split('.')

        try {
            if (payload) {
                const decodedPayload: Record<string, unknown> = JSON.parse(Buffer.from(payload, 'base64url').toString('utf8'))

                // Reject expired tokens even without exposing unverified claims.
                if (typeof decodedPayload.exp === 'number' && decodedPayload.exp < Math.floor(Date.now() / 1000)) {
                    Tach.processLogger.warn('Rejected expired JWT', { exp: decodedPayload.exp })
                    return undefined
                }
            }
        } catch {
            Tach.processLogger.warn('Failed to decode JWT payload; exposing raw bearer token only')
        }

        return { token, verified: false as const }
    }

    /**
     * Registers all discovered routes as Bun server route handlers.
     * Must be called after {@link Router.validateRoutes}.
     */
    static createServerRoutes() {
        if (!Router.allRoutes.has('/')) Router.allRoutes.set('/', new Set(['GET']))

        for (const [route, methods] of Router.allRoutes) {

            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            const serverRoute = async (request?: BunRequest, server?: Server<any>) => {
                const start = Date.now()
                const path  = new URL(request!.url).pathname
                const method = request!.method
                const ipAddress = server?.requestIP(request!)?.address ?? '0.0.0.0'
                const requestId = Tach.getRequestId(request!)

                let res: Response | undefined
                let requestKind = 'handler'

                try {
                    if (request!.headers.get('accept')?.includes('text/html')) {
                        requestKind = 'frontend'
                        const frontendResponse = await Tach.frontendRequestHandler?.(request!)
                        if (frontendResponse) {
                            res = frontendResponse
                        } else {
                            res = new Response(
                                await Bun.file(`${import.meta.dir}/../runtime/shells/${Tach.HTML_SHELL}`).text(),
                                { status: 200, headers: { 'Content-Type': 'text/html' } }
                            )
                        }
                    } else if (method === "OPTIONS") {
                        requestKind = 'options'
                        res = new Response(
                            Bun.file(`${Router.routesPath}${route === '/' ? '' : route}/OPTIONS`),
                            { status: 200, headers: { 'Content-Type': 'application/json' } }
                        )
                    } else {
                        const context: RequestContext = {
                            requestId,
                            ipAddress,
                        }

                        if (Router.middleware?.before) {
                            const earlyResponse = await Router.middleware.before(request!, context)
                            if (earlyResponse) {
                                requestKind = 'middleware'
                                res = earlyResponse
                            }
                        }

                        if (!res) {
                            const { handler, stdin, config } = await Router.processRequest(request!, route)

                            context.bearer = Tach.getBearerContext(stdin.headers?.authorization)

                            res = await Tach.serveRequest(handler, stdin, context, config)

                            if (Router.middleware?.after) {
                                res = await Router.middleware.after(request!, res, context)
                            }
                        }
                    }

                } catch (err) {
                    if (err instanceof Response) {
                        res = err
                    } else {
                        Tach.processLogger.error('Unhandled request error', {
                            err,
                            requestId,
                            method,
                            path,
                            route,
                        })
                        res = Response.json({ error: 'Internal server error' }, { status: 500, headers: Router.headers })
                    }
                }

                if (!res) {
                    res = Response.json({ error: 'Internal server error' }, { status: 500, headers: Router.headers })
                }

                res = Tach.withRequestId(res, requestId)

                Tach.requestLogger.info('Request completed', {
                    requestId,
                    method,
                    path,
                    route,
                    kind: requestKind,
                    status: res.status,
                    durationMs: Date.now() - start,
                    ipAddress,
                })

                return res
            }

            for (const method of methods) {
                if (method !== 'HTML') {
                    if (!Router.reqRoutes[route] || !Router.reqRoutes[`${route}/*`]) {
                        Router.reqRoutes[route]        = {}
                        Router.reqRoutes[`${route}/*`] = {}
                    }

                    Router.reqRoutes[route][method]        = serverRoute
                    Router.reqRoutes[`${route}/*`][method] = serverRoute
                }
            }

            if (Router.reqRoutes['//*']) delete Router.reqRoutes['//*']
        }
    }
}
