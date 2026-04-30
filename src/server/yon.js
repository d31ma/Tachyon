// @ts-check
import path from 'path';
import TTID from '@d31ma/ttid';
import Router from "./http/route-handler.js";
import Pool from "./process/process-pool.js";
import Validate from "./http/schema-validator.js";
import OpenAPI from "./openapi/openapi.js";
import FyloBrowser from "./fylo-browser/fylo-browser.js";
import Telemetry from './observability/telemetry.js';
import { withPublicBrowserEnv } from './http/browser-env.js';
import logger from './observability/logger.js';

/**
 * @typedef {import("bun").BunRequest} BunRequest
 * @typedef {import("bun").Server<any>} BunServer
 * @typedef {Record<string, string>} StringMap
 *
 * @typedef {object} RequestContext
 * @property {string} requestId
 * @property {string} [traceId]
 * @property {string} [spanId]
 * @property {string} [traceFlags]
 * @property {string} [traceState]
 * @property {string} ipAddress
 * @property {string} protocol
 * @property {string} host
 * @property {{ token: string, verified: false }} [bearer]
 *
 * @typedef {object} RequestPayload
 * @property {StringMap} [headers]
 * @property {Record<string, unknown>} [paths]
 * @property {unknown} [body]
 * @property {Record<string, unknown>} [query]
 *
 * @typedef {Record<string, unknown> & {
 *   request?: RequestPayload,
 *   response?: Record<number, unknown>
 * }} RouteOptions
 *
 * @typedef {object} RouteResponse
 * @property {number} status
 * @property {string} [body]
 *
 * @typedef {object} ForwardedHeader
 * @property {string} [for]
 * @property {string} [proto]
 * @property {string} [host]
 *
 * @typedef {object} RateLimitDecision
 * @property {boolean} allowed
 * @property {number} limit
 * @property {number} remaining
 * @property {number} resetAt
 * @property {StringMap} [headers]
 *
 * @typedef {(request: Request) => Promise<Response | null>} FrontendRequestHandler
 */
export default class Yon {
    static processLogger = logger.child({ scope: 'yon' });
    static requestLogger = logger.child({ scope: 'http' });
    static handlerLogger = logger.child({ scope: 'handler' });
    static STREAM_MIME_TYPE = "text/event-stream";
    static REQUEST_ID_HEADER = 'X-Request-Id';
    static MAX_REQUEST_ID_LENGTH = 200;
    static healthRoutePaths = ['/health', '/healthz'];
    static readyRoutePaths = ['/ready', '/readyz'];
    /** @type {FrontendRequestHandler | null} */
    static frontendRequestHandler = null;
    /** @type {Map<string, { count: number, resetAt: number }>} */
    static rateLimitBuckets = new Map();
    /**
     * Shared decoder instance — safe to reuse since we call decode() without
     * the `{stream: true}` option, making each call stateless.
     */
    static decoder = new TextDecoder();
    /**
     * Drains a readable byte stream line-by-line, calling `onLine` for each
     * complete newline-terminated line, and returns any trailing partial line.
     *
     * Reading stdout and stderr through this helper concurrently (via
     * `Promise.all`) prevents the classic pipe-buffer deadlock where a
     * subprocess blocks writing to stderr while the parent is still draining
     * stdout.
     *
     * @param {ReadableStream<Uint8Array>} stream - The byte stream to consume
     * @param {(line: string) => void} onLine - Called for each complete line (excluding the newline)
     * @returns {Promise<string>} The final partial line (the response body fragment)
     */
    static async drainStream(stream, onLine) {
        let partial = "";
        for await (const chunk of stream) {
            const combined = partial + Yon.decoder.decode(chunk);
            const lines = combined.split('\n');
            partial = lines.pop() ?? "";
            for (const line of lines) {
                if (line.length > 0)
                    onLine(line);
            }
        }
        return partial;
    }
    /** @param {string | undefined} value */
    static byteLength(value) {
        return value ? Buffer.byteLength(value) : 0;
    }
    /** @param {string | undefined} stderr */
    static handlerErrorBody(stderr) {
        if (process.env.YON_DEV === 'true' && process.env.YON_DEV_ERROR_DETAILS === 'true' && stderr) {
            return JSON.stringify({ detail: stderr });
        }
        return JSON.stringify({ detail: 'Internal server error' });
    }
    /** @param {string} shellHTML */
    static withMainStylesheet(shellHTML) {
        if (!Router.reqRoutes['/imports.css'] || shellHTML.includes('/imports.css'))
            return shellHTML;
        return shellHTML.replace('</head>', '    <link rel="stylesheet" href="/imports.css">\n</head>');
    }
    /**
     * @param {{ includeHotReloadClient?: boolean }} [options]
     * @returns {Promise<string>}
     */
    static async renderShellHTML(options = {}) {
        const includeHotReloadClient = options.includeHotReloadClient === true;
        const fyloBrowserPath = process.env.YON_DATA_BROWSER_PATH || '/_fylo';
        let shellHTML = await Bun.file(`${import.meta.dir}/../runtime/shells/app.html`).text();
        shellHTML = shellHTML
            .replace('<!--__TACHYON_DEV_HEAD__-->', includeHotReloadClient
                ? '    <script type="module" src="/hot-reload-client.js"></script>'
                : '')
            .replace('__FYLO_BROWSER_PATH__', fyloBrowserPath);
        return withPublicBrowserEnv(Yon.withMainStylesheet(shellHTML));
    }

    /**
     * @param {Request} request
     * @returns {boolean}
     */
    static isDocumentRequest(request) {
        const accept = request.headers.get('accept') || '';
        if (accept.includes('text/html'))
            return true;
        const fetchDest = (request.headers.get('sec-fetch-dest') || '').toLowerCase();
        if (fetchDest === 'document' || fetchDest === 'iframe')
            return true;
        const fetchMode = (request.headers.get('sec-fetch-mode') || '').toLowerCase();
        return fetchMode === 'navigate';
    }

    /**
     * @param {string | undefined} body
     * @returns {boolean}
     */
    static looksLikeJson(body) {
        if (!body)
            return false;
        const trimmed = body.trim();
        if (!trimmed)
            return false;
        if (!/^[\[{\"0-9tfn-]/.test(trimmed))
            return false;
        try {
            JSON.parse(trimmed);
            return true;
        }
        catch {
            return false;
        }
    }

    /**
     * @param {string | undefined} body
     * @returns {string | null}
     */
    static inferResponseContentType(body) {
        if (body === undefined)
            return null;
        if (Yon.looksLikeJson(body))
            return 'application/json; charset=utf-8';
        return 'text/plain; charset=utf-8';
    }
    /**
     * @param {string | null | undefined} value
     * @returns {string[]}
     */
    static splitHeaderList(value) {
        return value
            ?.split(',')
            .map(item => item.trim())
            .filter(Boolean)
            ?? [];
    }
    /**
     * @param {string | null} value
     * @returns {ForwardedHeader}
     */
    static parseForwardedHeader(value) {
        if (!value)
            return {};
        const firstEntry = value.split(',')[0]?.trim();
        if (!firstEntry)
            return {};
        const params = firstEntry.split(';');
        /** @type {ForwardedHeader} */
        const forwarded = {};
        for (const param of params) {
            const [rawKey, rawValue] = param.split('=');
            const key = rawKey?.trim().toLowerCase();
            const value = rawValue?.trim().replace(/^"|"$/g, '');
            if (!key || !value)
                continue;
            if (key === 'for')
                forwarded.for = value.replace(/^\[|\]$/g, '');
            if (key === 'proto')
                forwarded.proto = value;
            if (key === 'host')
                forwarded.host = value;
        }
        return forwarded;
    }
    /** @param {string} address */
    static isLoopbackAddress(address) {
        return address === '127.0.0.1' || address === '::1' || address === 'localhost';
    }
    /** @returns {string[]} */
    static trustedProxyEntries() {
        return Yon.splitHeaderList(process.env.YON_TRUST_PROXY);
    }
    /** @param {string} address */
    static isTrustedProxy(address) {
        const trustedEntries = Yon.trustedProxyEntries();
        if (trustedEntries.length === 0)
            return false;
        if (trustedEntries.includes('true') || trustedEntries.includes('*'))
            return true;
        if (trustedEntries.includes('loopback') && Yon.isLoopbackAddress(address))
            return true;
        return trustedEntries.includes(address);
    }
    /**
     * @param {Request} request
     * @param {string | null | undefined} remoteAddress
     * @returns {Pick<RequestContext, 'ipAddress' | 'protocol' | 'host'>}
     */
    static getClientInfo(request, remoteAddress) {
        const url = new URL(request.url);
        const fallback = {
            ipAddress: remoteAddress || '0.0.0.0',
            protocol: url.protocol.replace(/:$/, ''),
            host: url.host,
        };
        if (!remoteAddress || !Yon.isTrustedProxy(remoteAddress))
            return fallback;
        const forwarded = Yon.parseForwardedHeader(request.headers.get('forwarded'));
        const xForwardedFor = Yon.splitHeaderList(request.headers.get('x-forwarded-for'))[0];
        const xForwardedProto = request.headers.get('x-forwarded-proto')?.split(',')[0]?.trim();
        const xForwardedHost = request.headers.get('x-forwarded-host')?.split(',')[0]?.trim();
        return {
            ipAddress: forwarded.for || xForwardedFor || fallback.ipAddress,
            protocol: forwarded.proto || xForwardedProto || fallback.protocol,
            host: forwarded.host || xForwardedHost || fallback.host,
        };
    }
    /** @param {string} pathname */
    static isHealthEndpoint(pathname) {
        return Yon.healthRoutePaths.includes(pathname);
    }
    /** @param {string} pathname */
    static isReadyEndpoint(pathname) {
        return Yon.readyRoutePaths.includes(pathname);
    }
    /** @returns {{ max: number, windowMs: number }} */
    static getRateLimitConfig() {
        return {
            max: Number(process.env.YON_RATE_LIMIT_MAX || 0),
            windowMs: Number(process.env.YON_RATE_LIMIT_WINDOW_MS || 0),
        };
    }
    /**
     * @param {Pick<RateLimitDecision, 'limit' | 'remaining' | 'resetAt'>} decision
     * @param {number} [now]
     * @returns {Record<string, string>}
     */
    static getRateLimitHeaders(decision, now = Date.now()) {
        return {
            'RateLimit-Limit': String(Math.max(decision.limit, 0)),
            'RateLimit-Remaining': String(Math.max(decision.remaining, 0)),
            'RateLimit-Reset': String(Math.max(Math.ceil((decision.resetAt - now) / 1000), 0)),
        };
    }
    /**
     * @param {string} clientIpAddress
     * @returns {RateLimitDecision | null}
     */
    static takeInMemoryRateLimit(clientIpAddress) {
        const { max, windowMs } = Yon.getRateLimitConfig();
        if (max <= 0 || windowMs <= 0)
            return null;
        const now = Date.now();
        const existing = Yon.rateLimitBuckets.get(clientIpAddress);
        const bucket = !existing || existing.resetAt <= now
            ? { count: 0, resetAt: now + windowMs }
            : existing;
        bucket.count += 1;
        Yon.rateLimitBuckets.set(clientIpAddress, bucket);
        return {
            allowed: bucket.count <= max,
            limit: max,
            remaining: Math.max(max - bucket.count, 0),
            resetAt: bucket.resetAt,
        };
    }
    /**
     * @param {Request} request
     * @param {RequestContext} context
     * @param {string} pathname
     * @returns {Promise<{ rejection?: Response, headers?: Record<string, string> }>}
     */
    static async takeRateLimit(request, context, pathname) {
        if (Yon.isHealthEndpoint(pathname) || Yon.isReadyEndpoint(pathname)) {
            return {};
        }
        const decision = Router.rateLimiter
            ? await Router.rateLimiter.take(request, context) || null
            : Yon.takeInMemoryRateLimit(context.ipAddress);
        if (!decision)
            return {};
        const headers = {
            ...Yon.getRateLimitHeaders(decision),
            ...decision.headers,
        };
        if (!decision.allowed) {
            return {
                rejection: Response.json({ detail: 'Too many requests' }, {
                    status: 429,
                    headers: {
                        ...Router.getHeaders(request),
                        ...headers,
                        'Retry-After': headers['RateLimit-Reset'],
                    }
                }),
                headers,
            };
        }
        return { headers };
    }
    /**
     * @param {ReturnType<typeof Pool.acquireHandler>} proc
     * @param {RequestContext} context
     * @param {string} handler
     * @param {number} responseBytes
     * @param {number} errorBytes
     * @returns {Promise<Record<string, unknown>>}
     */
    static async logHandlerResourceUsage(proc, context, handler, responseBytes, errorBytes) {
        const exitCode = await proc.exited.catch(() => null);
        const usage = proc.resourceUsage();
        const loggedHandler = Yon.routeRelativeHandler(handler);
        if (!usage) {
            const summary = {
                'process.pid': proc.pid,
                'process.exit_code': exitCode,
                'tachyon.handler.response_bytes': responseBytes,
                'tachyon.handler.error_bytes': errorBytes,
            };
            Yon.handlerLogger.debug('Handler resource usage unavailable', {
                requestId: context.requestId,
                handler: loggedHandler,
                pid: proc.pid,
                exitCode,
                responseBytes,
                errorBytes,
            });
            return summary;
        }
        const summary = {
            'process.pid': proc.pid,
            'process.exit_code': exitCode,
            'process.cpu.user_us': usage.cpuTime.user,
            'process.cpu.system_us': usage.cpuTime.system,
            'process.cpu.total_us': usage.cpuTime.total,
            'process.memory.max_rss_bytes': usage.maxRSS,
            'process.fs.read_ops': usage.ops.in,
            'process.fs.write_ops': usage.ops.out,
            'process.context_switches.voluntary': usage.contextSwitches.voluntary,
            'process.context_switches.involuntary': usage.contextSwitches.involuntary,
            'tachyon.handler.response_bytes': responseBytes,
            'tachyon.handler.error_bytes': errorBytes,
        };
        Yon.handlerLogger.info('Handler resource usage', {
            requestId: context.requestId,
            handler: loggedHandler,
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
        });
        return summary;
    }
    /**
     * Executes a handler and returns a single {@link RouteResponse}.
     * stdout and stderr are drained concurrently to prevent pipe-buffer deadlocks.
     * @param {string[]} cmd
     * @param {RequestPayload} stdin
     * @param {RequestContext} context
     * @param {RouteOptions | undefined} _config
     * @returns {Promise<RouteResponse>}
     */
    static async getResponse(cmd, stdin, context, _config) {
        const handlerSpan = Telemetry.startChildSpan(context, `handler ${path.basename(cmd[0])}`, 'internal', context.requestId, {
            'code.file.path': cmd[0],
            'tachyon.handler.streaming': false,
        });
        const proc = Pool.acquireHandler(cmd[0]);
        proc.stdin.write(JSON.stringify({ ...stdin, context }));
        proc.stdin.end();
        const [out, err] = await Promise.all([
            Yon.drainStream(proc.stdout, line => Yon.handlerLogger.info('Handler stdout output', {
                requestId: context.requestId,
                pid: proc.pid,
                output: line,
            })),
            Yon.drainStream(proc.stderr, line => Yon.handlerLogger.error('Handler stderr output', {
                requestId: context.requestId,
                pid: proc.pid,
                output: line,
            })),
        ]);
        const usage = await Yon.logHandlerResourceUsage(proc, context, cmd[0], Yon.byteLength(out), Yon.byteLength(err));
        await Telemetry.endSpan(handlerSpan, {
            statusCode: err.length > 0 ? 500 : 200,
            attributes: usage,
        });
        if (out.length > 0)
            return { status: 200, body: out };
        if (err.length > 0)
            return { status: 500, body: Yon.handlerErrorBody(err) };
        return { status: 200 };
    }
    /**
     * Executes a handler and yields {@link RouteResponse} chunks as they arrive.
     * stderr is drained concurrently in the background so it never blocks stdout.
     * @param {string[]} cmd
     * @param {RequestPayload} stdin
     * @param {RequestContext} context
     * @param {RouteOptions | undefined} _config
     * @returns {AsyncGenerator<RouteResponse>}
     */
    static async *getStreamResponse(cmd, stdin, context, _config) {
        const handlerSpan = Telemetry.startChildSpan(context, `handler ${path.basename(cmd[0])}`, 'internal', context.requestId, {
            'code.file.path': cmd[0],
            'tachyon.handler.streaming': true,
        });
        const proc = Pool.acquireHandler(cmd[0]);
        proc.stdin.write(JSON.stringify({ ...stdin, context }));
        proc.stdin.end();
        // Drain stderr concurrently in the background so its OS buffer never fills
        // and blocks the subprocess while we are processing stdout chunks.
        /** @type {string[]} */
        const stderrLines = [];
        let responseBytes = 0;
        const stderrDone = Yon.drainStream(proc.stderr, line => {
            stderrLines.push(line);
            Yon.handlerLogger.error('Handler stderr output', {
                requestId: context.requestId,
                pid: proc.pid,
                output: line,
            });
        });
        for await (const chunk of proc.stdout) {
            const body = Yon.decoder.decode(chunk);
            const lines = body.split('\n');
            const partial = lines.pop() ?? "";
            for (const line of lines) {
                if (line.length > 0) {
                    Yon.handlerLogger.info('Handler stdout output', {
                        requestId: context.requestId,
                        pid: proc.pid,
                        output: line,
                    });
                }
            }
            responseBytes += Yon.byteLength(partial);
            if (partial.length > 0)
                yield { status: 200, body: partial };
        }
        const errRemainder = await stderrDone;
        const errorBytes = Yon.byteLength(errRemainder)
            + stderrLines.reduce((total, line) => total + Yon.byteLength(line), 0);
        const usage = await Yon.logHandlerResourceUsage(proc, context, cmd[0], responseBytes, errorBytes);
        await Telemetry.endSpan(handlerSpan, {
            statusCode: errRemainder.length > 0 || stderrLines.length > 0 ? 500 : 200,
            attributes: usage,
        });
        if (errRemainder.length > 0) {
            yield { status: 500, body: Yon.handlerErrorBody(errRemainder) };
        }
        else if (stderrLines.length > 0) {
            yield { status: 500, body: Yon.handlerErrorBody(stderrLines.join(' ')) };
        }
    }
    /**
     * @param {string | null | undefined} authorization
     * @returns {string | null}
     */
    static getBasicCredentials(authorization) {
        if (!authorization)
            return null;
        const [scheme, encoded] = authorization.split(' ');
        if (scheme?.toLowerCase() !== 'basic' || !encoded)
            return null;
        try {
            return atob(encoded);
        }
        catch {
            return null;
        }
    }
    /**
     * @param {string | null | undefined} authorization
     * @param {string | undefined} basicAuth
     * @param {string | undefined} basicAuthHash
     * @returns {Promise<boolean>}
     */
    static async isAuthorizedClient(authorization, basicAuth, basicAuthHash) {
        const provided = Yon.getBasicCredentials(authorization);
        if (!provided)
            return false;
        if (basicAuthHash) {
            return Bun.password.verify(provided, basicAuthHash);
        }
        if (!basicAuth)
            return false;
        // Timing-safe comparison prevents brute-force timing oracle attacks
        const a = Buffer.from(basicAuth);
        const b = Buffer.from(provided);
        if (a.length !== b.length)
            return false;
        return crypto.timingSafeEqual(a, b);
    }
    /** @param {FrontendRequestHandler | null} handler */
    static setFrontendRequestHandler(handler) {
        Yon.frontendRequestHandler = handler;
    }
    /**
     * @param {Request} request
     * @returns {string}
     */
    static getRequestId(request) {
        const incoming = request.headers.get(Yon.REQUEST_ID_HEADER)?.trim();
        if (incoming && incoming.length <= Yon.MAX_REQUEST_ID_LENGTH) {
            return incoming;
        }
        return TTID.generate();
    }
    /**
     * @param {string} handler
     * @returns {string}
     */
    static routeRelativeHandler(handler) {
        const relative = path.relative(Router.routesPath, handler).replaceAll(path.sep, '/');
        if (!relative.startsWith('..') && !path.isAbsolute(relative)) {
            return `/${relative}`;
        }
        return handler.replaceAll(path.sep, '/');
    }
    /**
     * @param {Response} response
     * @param {string} requestId
     * @returns {Response}
     */
    static withRequestId(response, requestId) {
        const headers = new Headers(response.headers);
        headers.set(Yon.REQUEST_ID_HEADER, requestId);
        return new Response(response.body, {
            status: response.status,
            statusText: response.statusText,
            headers,
        });
    }
    /**
     * @param {Response} response
     * @param {Record<string, string> | undefined} extraHeaders
     * @returns {Response}
     */
    static withAdditionalHeaders(response, extraHeaders) {
        if (!extraHeaders || Object.keys(extraHeaders).length === 0)
            return response;
        const headers = new Headers(response.headers);
        for (const [name, value] of Object.entries(extraHeaders)) {
            if (value)
                headers.set(name, value);
        }
        return new Response(response.body, {
            status: response.status,
            statusText: response.statusText,
            headers,
        });
    }
    /**
     * @param {Response} response
     * @param {Request} request
     * @returns {Response}
     */
    static withCacheControl(response, request) {
        if (response.headers.has('Cache-Control'))
            return response;
        const headers = new Headers(response.headers);
        headers.set('Cache-Control', Router.getCacheControlHeader(new URL(request.url).pathname, response.headers.get('Content-Type')));
        return new Response(response.body, {
            status: response.status,
            statusText: response.statusText,
            headers,
        });
    }
    /**
     * @param {Request} request
     * @returns {Response | null}
     */
    static rejectDisallowedOrigin(request) {
        if (Router.isOriginAllowed(request))
            return null;
        return Response.json({ detail: 'Origin not allowed' }, { status: 403, headers: Router.getHeaders(request) });
    }
    /**
     * @param {Request} request
     * @param {'ok' | 'ready'} kind
     * @returns {Response}
     */
    static healthResponse(request, kind) {
        return Response.json({
            status: kind,
            uptimeMs: Math.round(process.uptime() * 1000),
        }, {
            status: 200,
            headers: {
                ...Router.getHeaders(request),
                'Cache-Control': 'no-store',
            }
        });
    }
    /**
     * @param {Request} request
     * @param {string} handler
     * @param {RequestPayload} stdin
     * @param {RequestContext} context
     * @param {RouteOptions | undefined} config
     * @returns {Promise<Response>}
     */
    static async serveRequest(request, handler, stdin, context, config) {
        const responseHeaders = Router.getHeaders(request);
        if ((process.env.YON_BASIC_AUTH || process.env.YON_BASIC_AUTH_HASH)
            && !await Yon.isAuthorizedClient(stdin.headers?.authorization, process.env.YON_BASIC_AUTH, process.env.YON_BASIC_AUTH_HASH)) {
            return Response.json({ detail: "Unauthorized Client" }, { status: 401, headers: { ...responseHeaders, "WWW-Authenticate": 'Basic realm="Secure Area"' } });
        }
        if (process.env.YON_VALIDATE !== undefined) {
            try {
                await Validate.validateData(handler, "req", stdin);
            }
            catch (e) {
                const detail = e instanceof Error ? e.message : String(e);
                return Response.json({ detail }, { status: 400, headers: responseHeaders });
            }
        }
        if (stdin.headers?.accept === Yon.STREAM_MIME_TYPE) {
            const stream = new ReadableStream({
                async start(controller) {
                    for await (const { body, status } of Yon.getStreamResponse([handler], stdin, context, config)) {
                        if (process.env.YON_VALIDATE !== undefined) {
                            try {
                                await Validate.validateData(handler, status === 200 ? "res" : "err", body);
                            }
                            catch (e) {
                                const detail = e instanceof Error ? e.message : String(e);
                                controller.enqueue(JSON.stringify({ detail }));
                            }
                        }
                        else {
                            controller.enqueue(body);
                        }
                    }
                    controller.close();
                }
            });
            return new Response(stream, { headers: { ...responseHeaders, "Content-Type": Yon.STREAM_MIME_TYPE } });
        }
        const { body, status } = await Yon.getResponse([handler], stdin, context, config);
        const matchedStatus = body ? await Validate.matchStatusCode(handler, body) : null;
        const finalStatus = matchedStatus ?? status;
        if (process.env.YON_VALIDATE !== undefined) {
            const ioKey = matchedStatus ? String(matchedStatus) : (status === 200 ? "res" : "err");
            try {
                await Validate.validateData(handler, ioKey, body);
            }
            catch (e) {
                const detail = e instanceof Error ? e.message : String(e);
                return Response.json({ detail }, { status: 422, headers: responseHeaders });
            }
        }
        const inferredContentType = Yon.inferResponseContentType(body);
        const headers = new Headers(responseHeaders);
        if (inferredContentType && !headers.has('Content-Type'))
            headers.set('Content-Type', inferredContentType);
        return new Response(body, { status: finalStatus, headers });
    }
    /**
     * Extracts a Bearer token without exposing decoded claims to handlers.
     * The token is decoded only internally to reject expired JWTs when possible.
     * @param {string | null | undefined} authorization - The raw `Authorization` header value
     * @returns {{ token: string, verified: false } | undefined}
     */
    static getBearerContext(authorization) {
        if (!authorization)
            return undefined;
        const [authType, token] = authorization.split(' ');
        if (authType?.toLowerCase() !== "bearer" || !token)
            return undefined;
        const [, payload] = token.split('.');
        try {
            if (payload) {
                const decodedPayload = JSON.parse(Buffer.from(payload, 'base64url').toString('utf8'));
                // Reject expired tokens even without exposing unverified claims.
                if (typeof decodedPayload.exp === 'number' && decodedPayload.exp < Math.floor(Date.now() / 1000)) {
                    Yon.processLogger.warn('Rejected expired JWT', { exp: decodedPayload.exp });
                    return undefined;
                }
            }
        }
        catch {
            Yon.processLogger.warn('Failed to decode JWT payload; exposing raw bearer token only');
        }
        return { token, verified: false };
    }
    /**
     * Wraps an inline route handler (e.g. FYLO browser, OpenAPI docs) with the
     * same security and observability pipeline that file-system-routed handlers
     * receive: CORS origin rejection, rate limiting, middleware.before/after,
     * Basic Auth, telemetry spans, request-id propagation, and request logging.
     *
     * @param {(request: Request, server?: BunServer) => Promise<Response> | Response} handler
     * @param {{ route?: string }} [options]
     * @returns {(request?: Request, server?: BunServer) => Promise<Response>}
     */
    static wrapInlineRoute(handler, options = {}) {
        const routeLabel = options.route || 'inline';
        return async (request = new Request('http://localhost/'), server) => {
            const start = Date.now();
            const url = new URL(request.url);
            const pathname = url.pathname;
            const method = request.method;
            const clientInfo = Yon.getClientInfo(request, server?.requestIP(/** @type {BunRequest} */ (request))?.address ?? null);
            const requestId = Yon.getRequestId(request);
            /** @type {RequestContext} */
            const context = {
                requestId,
                ipAddress: clientInfo.ipAddress,
                protocol: clientInfo.protocol,
                host: clientInfo.host,
            };
            const requestSpan = Telemetry.startRequestSpan(request, {
                requestId,
                route: routeLabel,
                method,
                path: pathname,
                protocol: clientInfo.protocol,
                host: clientInfo.host,
                ipAddress: clientInfo.ipAddress,
            });
            if (requestSpan) {
                context.traceId = requestSpan.traceId;
                context.spanId = requestSpan.spanId;
                context.traceFlags = requestSpan.traceFlags;
                context.traceState = requestSpan.traceState;
            }
            /** @type {Response | undefined} */
            let res;
            let requestKind = 'inline';
            /** @type {Record<string, string> | undefined} */
            let responseHeaders;
            try {
                const originRejection = Yon.rejectDisallowedOrigin(request);
                if (originRejection) {
                    requestKind = 'cors';
                    res = originRejection;
                } else {
                    const rateLimit = await Yon.takeRateLimit(request, context, pathname);
                    responseHeaders = rateLimit.headers;
                    if (rateLimit.rejection) {
                        requestKind = 'rate-limit';
                        res = rateLimit.rejection;
                    }
                }
                if (!res && Router.middleware?.before) {
                    const earlyResponse = await Router.middleware.before(request, context);
                    if (earlyResponse) {
                        requestKind = 'middleware';
                        res = earlyResponse;
                    }
                }
                if (!res && (process.env.YON_BASIC_AUTH || process.env.YON_BASIC_AUTH_HASH)) {
                    const authorization = request.headers.get('authorization');
                    if (!await Yon.isAuthorizedClient(authorization, process.env.YON_BASIC_AUTH, process.env.YON_BASIC_AUTH_HASH)) {
                        res = Response.json(
                            { detail: 'Unauthorized Client' },
                            { status: 401, headers: { ...Router.getHeaders(request), 'WWW-Authenticate': 'Basic realm="Secure Area"' } },
                        );
                    }
                }
                if (!res) {
                    res = await handler(request, server);
                    if (Router.middleware?.after) {
                        res = await Router.middleware.after(request, res, context);
                    }
                }
            } catch (err) {
                if (err instanceof Response) {
                    res = err;
                } else {
                    Yon.processLogger.error('Unhandled inline route error', {
                        err,
                        requestId,
                        method,
                        path: pathname,
                        route: routeLabel,
                    });
                    res = Response.json({ error: 'Internal server error' }, { status: 500, headers: Router.getHeaders(request) });
                }
            }
            if (!res) {
                res = Response.json({ error: 'Internal server error' }, { status: 500, headers: Router.getHeaders(request) });
            }
            res = Yon.withAdditionalHeaders(res, responseHeaders);
            res = Yon.withCacheControl(res, request);
            res = Yon.withRequestId(res, requestId);
            res = Telemetry.withTraceHeaders(res, requestSpan);
            await Telemetry.endSpan(requestSpan, {
                statusCode: res.status,
                attributes: {
                    'tachyon.request.kind': requestKind,
                    'http.response.status_code': res.status,
                    'http.response.cache_control': res.headers.get('cache-control') ?? undefined,
                },
            });
            Yon.requestLogger.info('Request completed', {
                requestId,
                traceId: context.traceId,
                method,
                path: pathname,
                route: routeLabel,
                kind: requestKind,
                status: res.status,
                durationMs: Date.now() - start,
                ipAddress: clientInfo.ipAddress,
            });
            return res;
        };
    }
    /**
     * Registers all discovered routes as Bun server route handlers.
     * Must be called after {@link Router.validateRoutes}.
     */
    static createServerRoutes() {
        /** @type {(handler: (request: Request, server?: BunServer) => Promise<Response> | Response, options?: { route?: string }) => (request?: Request, server?: BunServer) => Promise<Response>} */
        const wrapRoute = (handler, options) => Yon.wrapInlineRoute(handler, options);
        OpenAPI.registerRoutes();
        FyloBrowser.registerRoutes(wrapRoute);
        for (const healthPath of [...Yon.healthRoutePaths, ...Yon.readyRoutePaths]) {
            if (!Router.reqRoutes[healthPath])
                Router.reqRoutes[healthPath] = {};
        }
        for (const healthPath of Yon.healthRoutePaths) {
            if (!Router.reqRoutes[healthPath].GET) {
                Router.reqRoutes[healthPath].GET = (request = new Request(`http://localhost${healthPath}`)) => Yon.healthResponse(request, 'ok');
            }
        }
        for (const readyPath of Yon.readyRoutePaths) {
            if (!Router.reqRoutes[readyPath].GET) {
                Router.reqRoutes[readyPath].GET = (request = new Request(`http://localhost${readyPath}`)) => Yon.healthResponse(request, 'ready');
            }
        }
        for (const [route, methods] of Router.allRoutes) {
            /** @type {(request?: Request, server?: BunServer) => Promise<Response>} */
            const serverRoute = async (request = new Request('http://localhost/'), server) => {
                const start = Date.now();
                const path = new URL(request.url).pathname;
                const method = request.method;
                const clientInfo = Yon.getClientInfo(request, server?.requestIP(/** @type {BunRequest} */ (request))?.address ?? null);
                const requestId = Yon.getRequestId(request);
                /** @type {RequestContext} */
                const context = {
                    requestId,
                    ipAddress: clientInfo.ipAddress,
                    protocol: clientInfo.protocol,
                    host: clientInfo.host,
                };
                const requestSpan = Telemetry.startRequestSpan(request, {
                    requestId,
                    route,
                    method,
                    path,
                    protocol: clientInfo.protocol,
                    host: clientInfo.host,
                    ipAddress: clientInfo.ipAddress,
                });
                if (requestSpan) {
                    context.traceId = requestSpan.traceId;
                    context.spanId = requestSpan.spanId;
                    context.traceFlags = requestSpan.traceFlags;
                    context.traceState = requestSpan.traceState;
                }
                /** @type {Response | undefined} */
                let res;
                let requestKind = 'handler';
                /** @type {Record<string, string> | undefined} */
                let responseHeaders;
                try {
                    const originRejection = Yon.rejectDisallowedOrigin(request);
                    if (originRejection) {
                        requestKind = 'cors';
                        res = originRejection;
                    }
                    else {
                        const rateLimit = await Yon.takeRateLimit(request, context, path);
                        responseHeaders = rateLimit.headers;
                        if (rateLimit.rejection) {
                            requestKind = 'rate-limit';
                            res = rateLimit.rejection;
                        }
                    }
                    if (res) {
                        // no-op, request already handled
                    }
                    else if ((method === 'GET' || method === 'HEAD')
                        && Yon.isDocumentRequest(request)
                        && Router.hasPageRoute(path)) {
                        requestKind = 'frontend';
                        const frontendResponse = await Yon.frontendRequestHandler?.(request);
                        if (frontendResponse) {
                            res = frontendResponse;
                        }
                        else {
                            const shellHTML = await Yon.renderShellHTML({
                                includeHotReloadClient: process.env.NODE_ENV !== 'production'
                            });
                            res = new Response(shellHTML, { status: 200, headers: { 'Content-Type': 'text/html' } });
                        }
                    }
                    else if (method === "OPTIONS") {
                        requestKind = 'options';
                        const routePath = route === '/' ? '' : Router.routeToFilesystemPath(route);
                        res = new Response(Bun.file(`${Router.routesPath}${routePath}/${Router.optionsFileName}`), { status: 200, headers: { ...Router.getHeaders(request), 'Content-Type': 'application/json' } });
                    }
                    else {
                        if (Router.middleware?.before) {
                            const earlyResponse = await Router.middleware.before(request, context);
                            if (earlyResponse) {
                                requestKind = 'middleware';
                                res = earlyResponse;
                            }
                        }
                        if (!res) {
                            const { handler, stdin, config } = await Router.processRequest(/** @type {BunRequest} */ (request), route);
                            context.bearer = Yon.getBearerContext(stdin.headers?.authorization);
                            res = await Yon.serveRequest(request, handler, stdin, context, config);
                            if (Router.middleware?.after) {
                                res = await Router.middleware.after(request, res, context);
                            }
                        }
                    }
                }
                catch (err) {
                    if (err instanceof Response) {
                        res = err;
                    }
                    else {
                        Yon.processLogger.error('Unhandled request error', {
                            err,
                            requestId,
                            method,
                            path,
                            route,
                        });
                        res = Response.json({ error: 'Internal server error' }, { status: 500, headers: Router.getHeaders(request) });
                    }
                }
                if (!res) {
                    res = Response.json({ error: 'Internal server error' }, { status: 500, headers: Router.getHeaders(request) });
                }
                res = Yon.withAdditionalHeaders(res, responseHeaders);
                res = Yon.withCacheControl(res, request);
                res = Yon.withRequestId(res, requestId);
                res = Telemetry.withTraceHeaders(res, requestSpan);
                await Telemetry.endSpan(requestSpan, {
                    statusCode: res.status,
                    attributes: {
                        'tachyon.request.kind': requestKind,
                        'http.response.status_code': res.status,
                        'http.response.cache_control': res.headers.get('cache-control') ?? undefined,
                    },
                });
                Yon.requestLogger.info('Request completed', {
                    requestId,
                    traceId: context.traceId,
                    method,
                    path,
                    route,
                    kind: requestKind,
                    status: res.status,
                    durationMs: Date.now() - start,
                    ipAddress: clientInfo.ipAddress,
                });
                return res;
            };
            for (const method of methods) {
                if (!Router.reqRoutes[route] || !Router.reqRoutes[`${route}/*`]) {
                    Router.reqRoutes[route] = {};
                    Router.reqRoutes[`${route}/*`] = {};
                }
                Router.reqRoutes[route][method] = serverRoute;
                Router.reqRoutes[`${route}/*`][method] = serverRoute;
            }
            if (Router.reqRoutes['//*'])
                delete Router.reqRoutes['//*'];
        }
    }
}
