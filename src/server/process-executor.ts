import { BunRequest, Server } from "bun";
import { randomUUID, timingSafeEqual } from "node:crypto";
import Router, { RequestContext, RouteOptions, RequestPayload, RouteResponse, type RateLimitDecision } from "./route-handler.js";
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
    private static readonly healthRoutePaths = ['/health', '/healthz']
    private static readonly readyRoutePaths = ['/ready', '/readyz']
    private static frontendRequestHandler:
        | ((request: BunRequest) => Promise<Response | null>)
        | null = null
    private static readonly rateLimitBuckets = new Map<string, { count: number; resetAt: number }>()

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

    private static withMainStylesheet(shellHTML: string): string {
        if (!Router.reqRoutes['/main.css'] || shellHTML.includes('/main.css')) return shellHTML
        return shellHTML.replace('</head>', '    <link rel="stylesheet" href="/main.css">\n</head>')
    }

    private static splitHeaderList(value: string | null | undefined): string[] {
        return value
            ?.split(',')
            .map(item => item.trim())
            .filter(Boolean)
            ?? []
    }

    private static parseForwardedHeader(value: string | null): {
        for?: string
        proto?: string
        host?: string
    } {
        if (!value) return {}

        const firstEntry = value.split(',')[0]?.trim()
        if (!firstEntry) return {}

        const params = firstEntry.split(';')
        const forwarded: { for?: string; proto?: string; host?: string } = {}

        for (const param of params) {
            const [rawKey, rawValue] = param.split('=')
            const key = rawKey?.trim().toLowerCase()
            const value = rawValue?.trim().replace(/^"|"$/g, '')

            if (!key || !value) continue

            if (key === 'for') forwarded.for = value.replace(/^\[|\]$/g, '')
            if (key === 'proto') forwarded.proto = value
            if (key === 'host') forwarded.host = value
        }

        return forwarded
    }

    private static isLoopbackAddress(address: string): boolean {
        return address === '127.0.0.1' || address === '::1' || address === 'localhost'
    }

    private static trustedProxyEntries(): string[] {
        return Tach.splitHeaderList(process.env.TRUST_PROXY)
    }

    private static isTrustedProxy(address: string): boolean {
        const trustedEntries = Tach.trustedProxyEntries()

        if (trustedEntries.length === 0) return false
        if (trustedEntries.includes('true') || trustedEntries.includes('*')) return true
        if (trustedEntries.includes('loopback') && Tach.isLoopbackAddress(address)) return true

        return trustedEntries.includes(address)
    }

    static getClientInfo(
        request: Request,
        remoteAddress?: string | null,
    ): Pick<RequestContext, 'ipAddress' | 'protocol' | 'host'> {
        const url = new URL(request.url)
        const fallback = {
            ipAddress: remoteAddress || '0.0.0.0',
            protocol: url.protocol.replace(/:$/, ''),
            host: url.host,
        }

        if (!remoteAddress || !Tach.isTrustedProxy(remoteAddress)) return fallback

        const forwarded = Tach.parseForwardedHeader(request.headers.get('forwarded'))
        const xForwardedFor = Tach.splitHeaderList(request.headers.get('x-forwarded-for'))[0]
        const xForwardedProto = request.headers.get('x-forwarded-proto')?.split(',')[0]?.trim()
        const xForwardedHost = request.headers.get('x-forwarded-host')?.split(',')[0]?.trim()

        return {
            ipAddress: forwarded.for || xForwardedFor || fallback.ipAddress,
            protocol: forwarded.proto || xForwardedProto || fallback.protocol,
            host: forwarded.host || xForwardedHost || fallback.host,
        }
    }

    private static isHealthEndpoint(pathname: string): boolean {
        return Tach.healthRoutePaths.includes(pathname)
    }

    private static isReadyEndpoint(pathname: string): boolean {
        return Tach.readyRoutePaths.includes(pathname)
    }

    private static getRateLimitConfig() {
        return {
            max: Number(process.env.RATE_LIMIT_MAX || 0),
            windowMs: Number(process.env.RATE_LIMIT_WINDOW_MS || 0),
        }
    }

    private static getRateLimitHeaders(
        decision: Pick<RateLimitDecision, 'limit' | 'remaining' | 'resetAt'>,
        now = Date.now(),
    ): Record<string, string> {
        return {
            'RateLimit-Limit': String(Math.max(decision.limit, 0)),
            'RateLimit-Remaining': String(Math.max(decision.remaining, 0)),
            'RateLimit-Reset': String(Math.max(Math.ceil((decision.resetAt - now) / 1000), 0)),
        }
    }

    private static takeInMemoryRateLimit(
        clientIpAddress: string,
    ): RateLimitDecision | null {
        const { max, windowMs } = Tach.getRateLimitConfig()

        if (max <= 0 || windowMs <= 0) return null

        const now = Date.now()
        const existing = Tach.rateLimitBuckets.get(clientIpAddress)
        const bucket = !existing || existing.resetAt <= now
            ? { count: 0, resetAt: now + windowMs }
            : existing

        bucket.count += 1
        Tach.rateLimitBuckets.set(clientIpAddress, bucket)

        return {
            allowed: bucket.count <= max,
            limit: max,
            remaining: Math.max(max - bucket.count, 0),
            resetAt: bucket.resetAt,
        }
    }

    private static async takeRateLimit(
        request: Request,
        context: RequestContext,
        pathname: string,
    ): Promise<{ rejection?: Response; headers?: Record<string, string> }> {
        if (Tach.isHealthEndpoint(pathname) || Tach.isReadyEndpoint(pathname)) {
            return {}
        }

        const decision = Router.rateLimiter
            ? await Router.rateLimiter.take(request, context) || null
            : Tach.takeInMemoryRateLimit(context.ipAddress)

        if (!decision) return {}

        const headers = {
            ...Tach.getRateLimitHeaders(decision),
            ...decision.headers,
        }

        if (!decision.allowed) {
            return {
                rejection: Response.json(
                    { detail: 'Too many requests' },
                    {
                        status: 429,
                        headers: {
                            ...Router.getHeaders(request),
                            ...headers,
                            'Retry-After': headers['RateLimit-Reset'],
                        }
                    }
                ),
                headers,
            }
        }

        return { headers }
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

    private static getBasicCredentials(authorization: string | undefined): string | null {
        if (!authorization) return null

        const [scheme, encoded] = authorization.split(' ')

        if (scheme?.toLowerCase() !== 'basic' || !encoded) return null

        try {
            return atob(encoded)
        } catch {
            return null
        }
    }

    private static async isAuthorizedClient(
        authorization: string | undefined,
        basicAuth: string | undefined,
        basicAuthHash: string | undefined,
    ): Promise<boolean> {
        const provided = Tach.getBasicCredentials(authorization)

        if (!provided) return false

        if (basicAuthHash) {
            return Bun.password.verify(provided, basicAuthHash)
        }

        if (!basicAuth) return false

        // Timing-safe comparison prevents brute-force timing oracle attacks
        const a = Buffer.from(basicAuth)
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

    private static withAdditionalHeaders(response: Response, extraHeaders?: Record<string, string>): Response {
        if (!extraHeaders || Object.keys(extraHeaders).length === 0) return response

        const headers = new Headers(response.headers)

        for (const [name, value] of Object.entries(extraHeaders)) {
            if (value) headers.set(name, value)
        }

        return new Response(response.body, {
            status: response.status,
            statusText: response.statusText,
            headers,
        })
    }

    private static withCacheControl(response: Response, request: Request): Response {
        if (response.headers.has('Cache-Control')) return response

        const headers = new Headers(response.headers)
        headers.set(
            'Cache-Control',
            Router.getCacheControlHeader(
                new URL(request.url).pathname,
                response.headers.get('Content-Type'),
            )
        )

        return new Response(response.body, {
            status: response.status,
            statusText: response.statusText,
            headers,
        })
    }

    private static rejectDisallowedOrigin(request: Request): Response | null {
        if (Router.isOriginAllowed(request)) return null

        return Response.json(
            { detail: 'Origin not allowed' },
            { status: 403, headers: Router.getHeaders(request) }
        )
    }

    private static healthResponse(request: Request, kind: 'ok' | 'ready') {
        return Response.json(
            {
                status: kind,
                uptimeMs: Math.round(process.uptime() * 1000),
            },
            {
                status: 200,
                headers: {
                    ...Router.getHeaders(request),
                    'Cache-Control': 'no-store',
                }
            }
        )
    }

    private static async serveRequest(
        request:  BunRequest,
        handler:  string,
        stdin:    RequestPayload,
        context:  RequestContext,
        config?:  RouteOptions,
    ): Promise<Response> {
        const responseHeaders = Router.getHeaders(request)

        if (
            (process.env.BASIC_AUTH || process.env.BASIC_AUTH_HASH)
            && !await Tach.isAuthorizedClient(
                stdin.headers?.authorization,
                process.env.BASIC_AUTH,
                process.env.BASIC_AUTH_HASH,
            )
        ) {
            return Response.json(
                { detail: "Unauthorized Client" },
                { status: 401, headers: { ...responseHeaders, "WWW-Authenticate": 'Basic realm="Secure Area"' } }
            )
        }

        if (process.env.VALIDATE !== undefined) {
            try {
                await Validate.validateData(handler, "req", stdin as Record<string, unknown>)
            } catch (e) {
                return Response.json({ detail: (e as Error).message }, { status: 400, headers: responseHeaders })
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

            return new Response(stream, { headers: { ...responseHeaders, "Content-Type": Tach.STREAM_MIME_TYPE } })
        }

        const { body, status } = await Tach.getResponse([handler], stdin, context, config)

        const matchedStatus = body ? Validate.matchStatusCode(handler, body) : null
        const finalStatus   = matchedStatus ?? status

        if (process.env.VALIDATE !== undefined) {
            const ioKey = matchedStatus ? String(matchedStatus) : (status === 200 ? "res" : "err")
            try {
                await Validate.validateData(handler, ioKey, body!)
            } catch (e) {
                return Response.json({ detail: (e as Error).message }, { status: 422, headers: responseHeaders })
            }
        }

        return new Response(body, { status: finalStatus, headers: responseHeaders })
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

        for (const healthPath of [...Tach.healthRoutePaths, ...Tach.readyRoutePaths]) {
            if (!Router.reqRoutes[healthPath]) Router.reqRoutes[healthPath] = {}
        }

        for (const healthPath of Tach.healthRoutePaths) {
            if (!Router.reqRoutes[healthPath].GET) {
                Router.reqRoutes[healthPath].GET = (request?: BunRequest) =>
                    Tach.healthResponse(request!, 'ok')
            }
        }

        for (const readyPath of Tach.readyRoutePaths) {
            if (!Router.reqRoutes[readyPath].GET) {
                Router.reqRoutes[readyPath].GET = (request?: BunRequest) =>
                    Tach.healthResponse(request!, 'ready')
            }
        }

        for (const [route, methods] of Router.allRoutes) {

            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            const serverRoute = async (request?: BunRequest, server?: Server<any>) => {
                const start = Date.now()
                const path  = new URL(request!.url).pathname
                const method = request!.method
                const clientInfo = Tach.getClientInfo(
                    request!,
                    server?.requestIP(request!)?.address ?? null,
                )
                const requestId = Tach.getRequestId(request!)
                const context: RequestContext = {
                    requestId,
                    ipAddress: clientInfo.ipAddress,
                    protocol: clientInfo.protocol,
                    host: clientInfo.host,
                }

                let res: Response | undefined
                let requestKind = 'handler'
                let responseHeaders: Record<string, string> | undefined

                try {
                    const originRejection = Tach.rejectDisallowedOrigin(request!)
                    if (originRejection) {
                        requestKind = 'cors'
                        res = originRejection
                    } else {
                        const rateLimit = await Tach.takeRateLimit(request!, context, path)
                        responseHeaders = rateLimit.headers

                        if (rateLimit.rejection) {
                            requestKind = 'rate-limit'
                            res = rateLimit.rejection
                        }
                    }

                    if (res) {
                        // no-op, request already handled
                    } else if (request!.headers.get('accept')?.includes('text/html')) {
                        requestKind = 'frontend'
                        const frontendResponse = await Tach.frontendRequestHandler?.(request!)
                        if (frontendResponse) {
                            res = frontendResponse
                        } else {
                            const shellHTML = Tach.withMainStylesheet(
                                await Bun.file(`${import.meta.dir}/../runtime/shells/${Tach.HTML_SHELL}`).text()
                            )
                            res = new Response(
                                shellHTML,
                                { status: 200, headers: { 'Content-Type': 'text/html' } }
                            )
                        }
                    } else if (method === "OPTIONS") {
                        requestKind = 'options'
                        res = new Response(
                            Bun.file(`${Router.routesPath}${route === '/' ? '' : route}/OPTIONS`),
                            { status: 200, headers: { ...Router.getHeaders(request!), 'Content-Type': 'application/json' } }
                        )
                    } else {
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

                            res = await Tach.serveRequest(request!, handler, stdin, context, config)

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
                        res = Response.json({ error: 'Internal server error' }, { status: 500, headers: Router.getHeaders(request!) })
                    }
                }

                if (!res) {
                    res = Response.json({ error: 'Internal server error' }, { status: 500, headers: Router.getHeaders(request!) })
                }

                res = Tach.withAdditionalHeaders(res, responseHeaders)
                res = Tach.withCacheControl(res, request!)
                res = Tach.withRequestId(res, requestId)

                Tach.requestLogger.info('Request completed', {
                    requestId,
                    method,
                    path,
                    route,
                    kind: requestKind,
                    status: res.status,
                    durationMs: Date.now() - start,
                    ipAddress: clientInfo.ipAddress,
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
