// @ts-check
import { stat } from 'fs/promises';
import path from 'path';
import Router from '../server/http/route-handler.js';
/**
 * @typedef {object} StaticPreviewOptions
 * @property {boolean} [allowRootFallback]
 * @property {number} [port]
 * @property {string} [hostname]
 */

/** @param {string} filePath */
async function pathExists(filePath) {
    try {
        const info = await stat(filePath);
        return info.isFile();
    }
    catch {
        return false;
    }
}
/** @param {string} distPath */
async function loadRoutes(distPath) {
    try {
        return /** @type {Record<string, Record<string, number>>} */ (await Bun.file(path.join(distPath, 'routes.json')).json());
    }
    catch {
        return {};
    }
}
/**
 * @param {string} pathname
 * @param {Record<string, Record<string, number>>} routes
 */
function resolveRouteTemplate(pathname, routes) {
    const segments = pathname.split('/').filter(Boolean);
    for (const [route, slugs] of Object.entries(routes)) {
        const routeSegments = route.split('/').filter(Boolean);
        if (routeSegments.length !== segments.length)
            continue;
        let matches = true;
        for (let index = 0; index < routeSegments.length; index++) {
            const segment = routeSegments[index];
            if (!(segment in slugs) && segment !== segments[index]) {
                matches = false;
                break;
            }
        }
        if (matches)
            return route;
    }
    return pathname;
}
/** @param {string} pathname */
function shouldTreatAsAsset(pathname) {
    const basename = path.posix.basename(pathname);
    return basename.includes('.');
}
/** @param {string} pathname */
function routeToPortablePath(pathname) {
    return pathname.replace(/(^|\/):([^/]+)/g, '$1_$2');
}
/**
 * @param {string} distPath
 * @param {string} pathname
 * @returns {string}
 */
function buildPreviewNotFoundMessage(distPath, pathname) {
    if (pathname === '/') {
        const expectedRoot = path.join(distPath, 'index.html');
        const expectedNested = path.join(distPath, '<route>', 'index.html');
        return [
            'No previewable file was found for "/".',
            `Dist path: ${distPath}`,
            `Expected root entry: ${expectedRoot}`,
            `Or a nested route export like: ${expectedNested}`,
            'If your app lives in a subdirectory, run `bun preview` from that app folder instead.',
        ].join('\n');
    }
    return `Not Found: ${pathname}`;
}
/**
 * @param {string} distPath
 * @param {string} pathname
 * @param {StaticPreviewOptions} [options]
 */
export async function resolvePreviewFile(distPath, pathname, options = {}) {
    const allowRootFallback = options.allowRootFallback ?? true;
    const normalized = pathname === '/' ? '/' : pathname.replace(/\/+$/, '') || '/';
    const routeTemplate = shouldTreatAsAsset(normalized)
        ? normalized
        : resolveRouteTemplate(normalized, await loadRoutes(distPath));
    const portable = routeToPortablePath(routeTemplate);
    const directFile = path.join(distPath, portable === '/' ? 'index.html' : portable.slice(1));
    if (await pathExists(directFile))
        return directFile;
    if (!shouldTreatAsAsset(normalized)) {
        const nestedIndex = path.join(distPath, portable === '/' ? 'index.html' : portable.slice(1), 'index.html');
        if (await pathExists(nestedIndex))
            return nestedIndex;
        if (allowRootFallback) {
            const rootIndex = path.join(distPath, 'index.html');
            if (await pathExists(rootIndex))
                return rootIndex;
        }
    }
    return null;
}
/**
 * @param {string} distPath
 * @param {Request} req
 * @param {StaticPreviewOptions} [options]
 */
export async function serveStaticPreviewRequest(distPath, req, options = {}) {
    if (req.method !== 'GET' && req.method !== 'HEAD')
        return null;
    const url = new URL(req.url);
    const filePath = await resolvePreviewFile(distPath, url.pathname, options);
    if (!filePath)
        return null;
    const file = Bun.file(filePath);
    const headers = new Headers();
    if (file.type)
        headers.set('Content-Type', file.type);
    headers.set('Cache-Control', Router.getCacheControlHeader(url.pathname, file.type));
    const body = req.method === 'HEAD' ? null : await file.bytes();
    return new Response(body, {
        headers
    });
}
/**
 * @param {string} distPath
 * @param {StaticPreviewOptions} [options]
 */
export async function createStaticPreviewServer(distPath, options = {}) {
    const server = Bun.serve({
        port: options.port ?? Number(process.env.YON_PORT || 3000),
        hostname: options.hostname ?? process.env.YON_HOST ?? '127.0.0.1',
        async fetch(req) {
            const url = new URL(req.url);
            return await serveStaticPreviewRequest(distPath, req)
                ?? new Response(buildPreviewNotFoundMessage(distPath, url.pathname), {
                    status: 404,
                    headers: {
                        'Content-Type': 'text/plain; charset=utf-8'
                    }
                });
        }
    });
    return server;
}
