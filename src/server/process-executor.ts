import { BunRequest, Server } from "bun";
import Router, { RequestContext, RouteOptions, RequestPayload, RouteResponse } from "./route-handler.js";
import Pool from "./process-pool.js";
import Validate from "./schema-validator.js";
import './console-logger.js'

export default class Tach {

    private static readonly STREAM_MIME_TYPE = "text/event-stream"

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
            Tach.drainStream(proc.stdout, line => console.info(line,  proc.pid)),
            Tach.drainStream(proc.stderr, line => console.error(line, proc.pid)),
        ])

        if (out.length > 0) return { status: 200, body: out }
        if (err.length > 0) return { status: 500, body: err }
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
        const stderrDone = Tach.drainStream(proc.stderr, line => {
            stderrLines.push(line)
            console.error(line, proc.pid)
        })

        for await (const chunk of proc.stdout) {
            const combined = Tach.decoder.decode(chunk)
            const lines    = combined.split('\n')
            const partial  = lines.pop()!

            for (const line of lines) {
                if (line.length > 0) console.info(line, proc.pid)
            }

            if (partial.length > 0) yield { status: 200, body: partial }
        }

        const errRemainder = await stderrDone

        if (errRemainder.length > 0) {
            yield { status: 500, body: errRemainder }
        } else if (stderrLines.length > 0) {
            yield { status: 500, body: JSON.stringify({ detail: stderrLines.join(' ') }) }
        }
    }

    private static isAuthorizedClient(authorization: string | undefined, basicAuth: string): boolean {
        if (authorization) {
            const [, hash] = authorization.split(' ')
            return hash === btoa(basicAuth)
        }
        return false
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

        if (process.env.VALIDATE !== undefined) {
            try {
                await Validate.validateData(handler, status === 200 ? "res" : "err", body!)
            } catch (e) {
                return Response.json({ detail: (e as Error).message }, { status: 422, headers: Router.headers })
            }
        }

        return new Response(body, { status, headers: Router.headers })
    }

    /**
     * Decodes a Bearer JWT from an Authorization header.
     * Returns `undefined` if absent, not Bearer, or the payload is malformed.
     * @param authorization - The raw `Authorization` header value
     */
    private static decodeJWT(authorization: string | undefined) {
        if (!authorization) return undefined

        const [authType, token] = authorization.split(' ')

        if (authType.toLowerCase() !== "bearer") return undefined

        const [header, payload, signature] = token.split('.')

        try {
            return {
                header:    JSON.parse(atob(header)),
                payload:   JSON.parse(atob(payload)),
                signature,
            }
        } catch {
            console.warn(`Failed to decode JWT — malformed base64 or JSON payload`, process.pid)
            return undefined
        }
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

                let res: Response

                try {
                    if (request!.headers.get('accept')?.includes('text/html')) {
                        return new Response(
                            await Bun.file(`${import.meta.dir}/../runtime/shells/development.html`).text(),
                            { status: 200, headers: { 'Content-Type': 'text/html' } }
                        )
                    }

                    if (request!.method === "OPTIONS") {
                        console.info(`${path} - OPTIONS - 200`, process.pid)
                        return new Response(
                            Bun.file(`${Router.routesPath}${route === '/' ? '' : route}/OPTIONS`),
                            { status: 200, headers: { 'Content-Type': 'application/json' } }
                        )
                    }

                    const context: RequestContext = {
                        ipAddress: server?.requestIP(request!)?.address ?? '0.0.0.0',
                    }

                    if (Router.middleware?.before) {
                        const earlyResponse = await Router.middleware.before(request!, context)
                        if (earlyResponse) return earlyResponse
                    }

                    const { handler, stdin, config } = await Router.processRequest(request!, route)

                    context.bearer = Tach.decodeJWT(stdin.headers?.authorization)

                    res = await Tach.serveRequest(handler, stdin, context, config)

                    if (Router.middleware?.after) {
                        res = await Router.middleware.after(request!, res, context)
                    }

                } catch (err) {
                    res = err instanceof Response
                        ? err
                        : Response.json({ detail: (err as Error).message }, { status: 400, headers: Router.headers })
                }

                console.info(`${path} - ${request!.method} - ${res.status} - ${Date.now() - start}ms`, process.pid)

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
