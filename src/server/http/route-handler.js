// @ts-check
import { existsSync } from "fs";
import path from "path";
export { default as Tac } from "../../runtime/tac.js";

/**
 * @typedef {import("bun").BunRequest} BunRequest
 * @typedef {import("bun").Server<any>} BunServer
 * @typedef {Record<string, string>} StringMap
 * @typedef {Record<string, number>} SlugMap
 * @typedef {string | number | boolean | null | undefined} ParamValue
 * @typedef {(req?: Request, server?: BunServer) => Promise<Response> | Response} RouteHandler
 *
 * @typedef {object} RequestContext
 * @property {string} requestId
 * @property {string} ipAddress
 * @property {string} protocol
 * @property {string} host
 * @property {{ token: string, verified: false }} [bearer]
 *
 * @typedef {object} RequestPayload
 * @property {StringMap} [headers]
 * @property {Record<string, ParamValue>} [paths]
 * @property {unknown} [body]
 * @property {Record<string, unknown>} [query]
 *
 * @typedef {Record<string, unknown> & {
 *   request?: RequestPayload,
 *   response?: Record<number, unknown>
 * }} RouteOptions
 *
 * @typedef {object} Middleware
 * @property {(request: Request, context: RequestContext) => Promise<Response | void> | Response | void} [before]
 * @property {(request: Request, response: Response, context: RequestContext) => Promise<Response> | Response} [after]
 *
 * @typedef {object} RateLimitDecision
 * @property {boolean} allowed
 * @property {number} limit
 * @property {number} remaining
 * @property {number} resetAt
 * @property {StringMap} [headers]
 *
 * @typedef {object} RateLimiter
 * @property {(request: Request, context: RequestContext) => Promise<RateLimitDecision | null | void> | RateLimitDecision | null | void} take
 */
export default class Router {
    static pageFileName = 'index.html';
    /**
     * @param {string} envName
     * @param {string[]} candidates
     * @returns {string}
     */
    static resolveWorkspacePath(envName, candidates) {
        const configured = process.env[envName];
        if (configured)
            return path.isAbsolute(configured) ? configured : path.join(process.cwd(), configured);
        for (const candidate of candidates) {
            const resolved = path.join(process.cwd(), candidate);
            if (existsSync(resolved))
                return resolved;
        }
        return path.join(process.cwd(), candidates[0]);
    }
    /** @type {Record<string, Record<string, RouteHandler>>} */
    static reqRoutes = {};
    /** @type {Map<string, Set<string>>} */
    static allRoutes = new Map();
    /** @type {Record<string, Record<string, string>>} */
    static routeHandlers = {};
    /** @type {Record<string, SlugMap>} */
    static routeSlugs = {};
    static routesPath = Router.resolveWorkspacePath('YON_ROUTES_PATH', ['server/routes', 'routes']);
    static pagesPath = Router.resolveWorkspacePath('YON_PAGES_PATH', ['browser/pages', 'routes']);
    static componentsPath = Router.resolveWorkspacePath('YON_COMPONENTS_PATH', ['browser/components', 'components']);
    static assetsPath = Router.resolveWorkspacePath('YON_ASSETS_PATH', ['browser/shared/assets', 'shared/assets', 'assets']);
    static sharedDataPath = Router.resolveWorkspacePath('YON_SHARED_DATA_PATH', ['browser/shared/data', 'shared/data']);
    static sharedScriptsPath = Router.resolveWorkspacePath('YON_SHARED_SCRIPTS_PATH', ['browser/shared/scripts']);
    static sharedStylesPath = Router.resolveWorkspacePath('YON_SHARED_STYLES_PATH', ['browser/shared/styles']);
    static middlewarePath = process.env.YON_MIDDLEWARE_PATH || `${process.cwd()}/middleware`;
    static optionsFileName = 'OPTIONS.json';
    /** @type {Middleware | null} */
    static middleware = null;
    /** @type {RateLimiter | null} */
    static rateLimiter = null;
    /** @type {Record<string, Record<string, RouteOptions>>} */
    static routeConfigs = {};
    static resetStaticState() {
        for (const key of Object.keys(Router.reqRoutes))
            delete Router.reqRoutes[key];
        Router.allRoutes.clear();
        for (const key of Object.keys(Router.routeHandlers))
            delete Router.routeHandlers[key];
        for (const key of Object.keys(Router.routeSlugs))
            delete Router.routeSlugs[key];
        for (const key of Object.keys(Router.routeConfigs))
            delete Router.routeConfigs[key];
        Router.middleware = null;
        Router.rateLimiter = null;
    }
    static allMethods = process.env.YON_ALLOW_METHODS
        ? process.env.YON_ALLOW_METHODS.split(',')
        : ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD'];

    /**
     * @param {string | undefined} value
     * @returns {string[]}
     */
    static splitConfigList(value) {
        return value
            ?.split(',')
            .map(item => item.trim())
            .filter(item => item.length > 0)
            ?? [];
    }
    static get allowedOrigins() {
        return Router.splitConfigList(process.env.YON_ALLOW_ORIGINS);
    }

    /**
     * @param {Request} request
     * @returns {boolean}
     */
    static isOriginAllowed(request) {
        const origin = request.headers.get('origin');
        if (!origin)
            return true;
        if (origin === new URL(request.url).origin)
            return true;
        const allowedOrigins = Router.allowedOrigins;
        if (allowedOrigins.length === 0)
            return true;
        return allowedOrigins.includes('*') || allowedOrigins.includes(origin);
    }

    /**
     * @param {Request} [request]
     * @returns {string}
     */
    static resolveAllowedOrigin(request) {
        const allowedOrigins = Router.allowedOrigins;
        if (allowedOrigins.length === 0)
            return '';
        if (allowedOrigins.includes('*'))
            return '*';
        const requestOrigin = request?.headers.get('origin');
        if (request && requestOrigin) {
            if (requestOrigin === new URL(request.url).origin)
                return requestOrigin;
            if (allowedOrigins.includes(requestOrigin))
                return requestOrigin;
        }
        return allowedOrigins.length === 1 ? allowedOrigins[0] : '';
    }

    /**
     * @param {Request} [request]
     * @returns {Record<string, string>}
     */
    static getHeaders(request) {
        const allowOrigin = Router.resolveAllowedOrigin(request);
        /** @type {Record<string, string>} */
        const headers = {
            "Access-Control-Allow-Headers": process.env.YON_ALLOW_HEADERS || "",
            "Access-Control-Allow-Origin": allowOrigin,
            "Access-Control-Allow-Credentials": process.env.YON_ALLOW_CREDENTIALS || "false",
            "Access-Control-Expose-Headers": process.env.YON_ALLOW_EXPOSE_HEADERS || "",
            "Access-Control-Max-Age": process.env.YON_ALLOW_MAX_AGE || "",
            "Access-Control-Allow-Methods": process.env.YON_ALLOW_METHODS || "",
            // Security headers
            "X-Frame-Options": "DENY",
            "X-Content-Type-Options": "nosniff",
            "Content-Security-Policy": process.env.YON_CONTENT_SECURITY_POLICY || "default-src 'self'",
            "Referrer-Policy": "strict-origin-when-cross-origin",
        };
        if (request?.headers.get('origin') && allowOrigin && allowOrigin !== '*') {
            headers.Vary = 'Origin';
        }
        if (process.env.YON_ENABLE_HSTS === 'true') {
            headers["Strict-Transport-Security"] = process.env.YON_HSTS_VALUE || "max-age=31536000; includeSubDomains";
        }
        return headers;
    }
    static get headers() {
        return Router.getHeaders();
    }

    /**
     * @param {string} route
     * @param {string | undefined} terminalSegment
     * @param {string[]} staticPaths
     * @returns {{ pathname: string, slugs: SlugMap }}
     */
    static validateSegmentPath(route, terminalSegment, staticPaths = []) {
        const paths = route.split('/' );
        const normalizedPaths = paths.map((segment) => segment.startsWith('_') ? `:${segment.slice(1)}` : segment);
        const slugPattern = /^:.*/;
        /** @type {SlugMap} */
        const slugs = {};
        if (slugPattern.test(normalizedPaths[0]))
            throw new Error(`Invalid route: '${route}' â€” route cannot start with a slug segment`);
        normalizedPaths.forEach((segment, idx) => {
            if (slugPattern.test(segment) && (slugPattern.test(normalizedPaths[idx - 1]) || slugPattern.test(normalizedPaths[idx + 1]))) {
                throw new Error(`Invalid route: '${route}' â€” consecutive slug segments are not allowed`);
            }
            if (slugPattern.test(segment))
                slugs[segment] = idx;
        });
        const staticPath = normalizedPaths.filter((segment) => !slugPattern.test(segment)).join(',');
        if (staticPaths.includes(staticPath))
            throw new Error(`Duplicate route: '${route}'`);
        staticPaths.push(staticPath);
        const actualTerminal = normalizedPaths.pop();
        if (actualTerminal !== terminalSegment) {
            throw new Error(`Invalid route: '${route}' â€” expected terminal segment '${terminalSegment}'`);
        }
        const pathname = `/${normalizedPaths.join('/')}` || '/';
        return { pathname, slugs };
    }

    /**
     * @param {string} pathname
     * @param {Record<string, SlugMap>} slugs
     * @returns {string | null}
     */
    static resolveRoutePattern(pathname, slugs) {
        if (slugs[pathname])
            return pathname;
        const pathnameSegments = pathname.split('/').filter(Boolean);
        let matchedRoute = null;
        let matchedLength = -1;
        for (const route of Object.keys(slugs)) {
            const routeSegments = route.split('/').filter(Boolean);
            if (routeSegments.length !== pathnameSegments.length)
                continue;
            let matches = true;
            for (let index = 0; index < routeSegments.length; index += 1) {
                const routeSegment = routeSegments[index];
                const pathnameSegment = pathnameSegments[index];
                if (!routeSegment.startsWith(':') && routeSegment !== pathnameSegment) {
                    matches = false;
                    break;
                }
            }
            if (matches && routeSegments.length > matchedLength) {
                matchedRoute = route;
                matchedLength = routeSegments.length;
            }
        }
        return matchedRoute;
    }

    /** @param {string} pathname */
    static resolvePageRoute(pathname) {
        return Router.resolveRoutePattern(pathname, Router.routeSlugs);
    }

    /** @param {string} pathname */
    static hasPageRoute(pathname) {
        return Router.resolvePageRoute(pathname) !== null;
    }

    /**
     * Canonical route paths use `:slug` segments for runtime matching, while
     * source files live on disk with `_slug` segments for Windows-safe paths.
     * @param {string} route
     * @returns {string}
     */
    static routeToFilesystemPath(route) {
        return route.replace(/(^|\/):([^/]+)/g, '$1_$2');
    }

    /**
     * @param {string} route
     * @returns {string}
     */
    static filesystemPathToRoute(route) {
        return route.replace(/(^|\/)_([^/]+)/g, '$1:$2');
    }

    /**
     * @param {string} pathname
     * @param {string | null | undefined} contentType
     * @returns {string}
     */
    static getCacheControlHeader(pathname, contentType) {
        const normalizedPath = pathname.split('?')[0] || '/';
        const base = path.posix.basename(normalizedPath);
        const type = (contentType || '').toLowerCase();
        if (type.includes('text/html'))
            return 'no-cache, must-revalidate';
        if (normalizedPath === '/routes.json' || normalizedPath === '/shells.json')
            return 'no-cache, must-revalidate';
        if (['browser-env.js', 'imports.js', 'imports.css', 'spa-renderer.js', 'hot-reload-client.js'].includes(base)) {
            return 'no-cache, must-revalidate';
        }
        if (/^chunk-[a-z0-9]+\./i.test(base))
            return 'public, max-age=31536000, immutable';
        if (normalizedPath.startsWith('/shared/assets/'))
            return 'public, max-age=3600';
        if (normalizedPath.startsWith('/shared/data/'))
            return 'no-cache, must-revalidate';
        if (normalizedPath.startsWith('/components/')
            || normalizedPath.startsWith('/pages/')
            || normalizedPath.startsWith('/modules/')) {
            return 'no-cache, must-revalidate';
        }
        return 'no-store';
    }
    /** Maximum bytes buffered from an inbound request body before returning 413. */
    static MAX_BODY_BYTES = Number(process.env.YON_MAX_BODY_BYTES) || 1_048_576;
    /**
     * Validates a route file path, registers slugs, and records allowed methods.
     * Throws if the route is malformed or a duplicate.
     * @param {string} route - Relative path from the routes directory (e.g. `api/:id/GET`)
     * @param {string[]} staticPaths - Accumulator used to detect duplicate static segments
     */
    static async validateRoute(route, staticPaths = []) {
        route = route.replaceAll('\\', '/');
        const routeFilePath = route;
        const routeFile = path.posix.basename(route);
        const routeMethod = routeFile.split('.', 1)[0]?.toUpperCase();
        const routeDirectory = path.posix.dirname(route);
        const routeForValidation = routeDirectory === '.'
            ? routeMethod
            : `${routeDirectory}/${routeMethod}`;
        const { pathname: routePathname } = Router.validateSegmentPath(routeForValidation, routeMethod, staticPaths);
        route = routePathname;
        const routeOptionsFile = Bun.file(`${Router.routesPath}${Router.routeToFilesystemPath(route)}/${Router.optionsFileName}`);
        if (!Router.allRoutes.has(route))
            Router.allRoutes.set(route, new Set());
        if (routeMethod) {
            Router.allRoutes.get(route)?.add(routeMethod);
            Router.routeHandlers[route] ??= {};
            Router.routeHandlers[route][routeMethod] = path.join(Router.routesPath, routeFilePath);
        }
        if (await routeOptionsFile.exists() && !Router.routeConfigs[route]) {
            Router.routeConfigs[route] = await routeOptionsFile.json();
            Router.allRoutes.get(route)?.add('OPTIONS');
        }
    }

    /**
     * @param {string} route
     * @param {string[]} staticPaths
     */
    static validatePageRoute(route, staticPaths = []) {
        route = route.replaceAll('\\', '/');
        if (path.posix.basename(route) !== Router.pageFileName) {
            throw new Error(`Invalid page route: '${route}' — expected page filename '${Router.pageFileName}'`);
        }
        const pageDir = Router.filesystemPathToRoute(path.posix.dirname(route));
        const terminalSegment = 'page-terminal';
        const virtualRoute = pageDir === '.' ? terminalSegment : `${pageDir}/${terminalSegment}`;
        const { pathname, slugs } = Router.validateSegmentPath(virtualRoute, terminalSegment, staticPaths);
        Router.routeSlugs[pathname] = slugs;
    }
    /**
     * Extracts headers, body, and query parameters from a request and resolves
     * the filesystem handler path for the matched route.
     * @param {BunRequest} request - The incoming Bun request
     * @param {string} route - The matched route pattern
     * @returns {Promise<{ handler: string, stdin: RequestPayload, config: RouteOptions | undefined }>} Handler path, parsed stdin payload, and optional route config
     */
    static async processRequest(request, route) {
        /** @type {RequestPayload} */
        const stdin = {
            paths: request.params
        };
        /** @type {RouteOptions | undefined} */
        let requestConfig;
        if (Router.routeConfigs[route]?.[request.method]) {
            requestConfig = Router.routeConfigs[route][request.method];
        }
        stdin.headers = request.headers.toJSON();
        const bodyBytes = await Router.readBodyBytes(request);
        if (bodyBytes.byteLength > 0) {
            const contentType = request.headers.get('content-type') ?? '';
            const bodyText = new TextDecoder().decode(bodyBytes);
            stdin.body = contentType.includes('json') ? JSON.parse(bodyText) : bodyText;
        }
        const searchParams = new URL(request.url).searchParams;
        if (searchParams.size > 0) {
            stdin.query = Router.parseKVParams(searchParams);
        }
        return {
            handler: Router.routeHandlers[route]?.[request.method]
                ?? `${Router.routesPath}${Router.routeToFilesystemPath(route)}/${request.method}`,
            stdin,
            config: requestConfig
        };
    }

    /**
     * @param {Request} request
     * @returns {Promise<Uint8Array>}
     */
    static async readBodyBytes(request) {
        const contentLength = request.headers.get('content-length');
        if (contentLength && Number(contentLength) > Router.MAX_BODY_BYTES) {
            throw Response.json({ error: 'Payload too large' }, { status: 413, headers: Router.getHeaders(request) });
        }
        if (!request.body)
            return new Uint8Array();
        const reader = request.body.getReader();
        /** @type {Uint8Array[]} */
        const chunks = [];
        let total = 0;
        while (true) {
            const { done, value } = await reader.read();
            if (done)
                break;
            if (!value)
                continue;
            total += value.byteLength;
            if (total > Router.MAX_BODY_BYTES) {
                await reader.cancel();
                throw Response.json({ error: 'Payload too large' }, { status: 413, headers: Router.getHeaders(request) });
            }
            chunks.push(value);
        }
        const body = new Uint8Array(total);
        let offset = 0;
        for (const chunk of chunks) {
            body.set(chunk, offset);
            offset += chunk.byteLength;
        }
        return body;
    }
    /** Maximum allowed length for any single route or query parameter value. */
    static MAX_PARAM_LENGTH = Number(process.env.YON_MAX_PARAM_LENGTH) || 1000;
    /**
     * Coerces an array of string path segments into their native types
     * (number, boolean, null, undefined, or string).
     * @param {string[]} input - Raw string segments from the URL path
     * @returns {ParamValue[]} Typed parameter values
     * @throws Response with status 400 if any segment exceeds MAX_PARAM_LENGTH
     */
    static parseParams(input) {
        /** @type {ParamValue[]} */
        const params = [];
        for (const param of input) {
            if (param.length > Router.MAX_PARAM_LENGTH) {
                throw Response.json({ error: 'Parameter too long' }, { status: 400 });
            }
            const num = Number(param);
            if (!Number.isNaN(num))
                params.push(num);
            else if (param === 'true')
                params.push(true);
            else if (param === 'false')
                params.push(false);
            else if (param === 'null')
                params.push(null);
            else if (param === 'undefined')
                params.push(undefined);
            else
                params.push(param);
        }
        return params;
    }
    /**
     * Parses key-value pairs from {@link URLSearchParams} or {@link FormData},
     * coercing each value to its native type where possible.
     * @param {URLSearchParams | FormData} input - The source key-value collection
     * @returns {Record<string, unknown>} A record of coerced parameter values
     */
    static parseKVParams(input) {
        /** @type {Record<string, unknown>} */
        const params = {};
        for (const [key, val] of input) {
            if (typeof val === "string" && val.length > Router.MAX_PARAM_LENGTH) {
                throw Response.json({ error: 'Parameter too long' }, { status: 400 });
            }
            if (typeof val === "string") {
                try {
                    const parsed = JSON.parse(val);
                    // Only accept primitive types and plain arrays/objects — reject prototypes
                    if (parsed !== null && typeof parsed === "object" && Object.getPrototypeOf(parsed) !== Object.prototype && !Array.isArray(parsed)) {
                        params[key] = val;
                    }
                    else {
                        params[key] = parsed;
                    }
                }
                catch {
                    const num = Number(val);
                    if (!Number.isNaN(num))
                        params[key] = num;
                    else if (val === 'true')
                        params[key] = true;
                    else if (val === 'false')
                        params[key] = false;
                    else if (val.includes(','))
                        params[key] = Router.parseParams(val.split(','));
                    else if (val === 'null')
                        params[key] = null;
                    if (params[key] === undefined)
                        params[key] = val;
                }
            }
            else
                params[key] = val;
        }
        return params;
    }
    /**
     * Scans the routes directory and validates every discovered route file.
     */
    static async validateRoutes() {
        if (!existsSync(Router.routesPath))
            return;
        const routeFileNames = Router.allMethods
            .filter((method) => method !== 'OPTIONS')
            .flatMap((method) => [method, `${method}.*`]);
        const routes = Array.from(new Bun.Glob(`**/{${routeFileNames.join(',')}}`).scanSync({ cwd: Router.routesPath }))
            .filter((route) => !route.replaceAll('\\', '/').split('/').includes('__pycache__'));
        for (const route of routes)
            await Router.validateRoute(route);
    }
    static async validatePageRoutes() {
        if (!existsSync(Router.pagesPath))
            return;
        const pages = Array.from(new Bun.Glob(`**/${Router.pageFileName}`).scanSync({ cwd: Router.pagesPath }));
        if (existsSync(path.join(Router.pagesPath, Router.pageFileName)) && !pages.includes(Router.pageFileName))
            pages.unshift(Router.pageFileName);
        /** @type {string[]} */
        const staticPaths = [];
        for (const page of pages)
            Router.validatePageRoute(page, staticPaths);
    }
}
