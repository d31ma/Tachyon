// @ts-check
import { afterEach, expect, test } from 'bun:test';
import Fylo from '@d31ma/fylo';
import Router from '../../src/server/http/route-handler.js';
import Yon from '../../src/server/yon.js';
const originalTrustProxy = process.env.YON_TRUST_PROXY;
const originalContentSecurityPolicy = process.env.YON_CONTENT_SECURITY_POLICY;
const originalRoutesPath = Router.routesPath;
afterEach(() => {
    if (originalTrustProxy === undefined)
        delete process.env.YON_TRUST_PROXY;
    else
        process.env.YON_TRUST_PROXY = originalTrustProxy;
    if (originalContentSecurityPolicy === undefined)
        delete process.env.YON_CONTENT_SECURITY_POLICY;
    else
        process.env.YON_CONTENT_SECURITY_POLICY = originalContentSecurityPolicy;
    Router.routesPath = originalRoutesPath;
});
test('cache policy marks HTML and stable runtime assets as no-cache', () => {
    expect(Router.getCacheControlHeader('/', 'text/html;charset=utf-8')).toBe('no-cache, must-revalidate');
    expect(Router.getCacheControlHeader('/imports.js', 'text/javascript;charset=utf-8')).toBe('no-cache, must-revalidate');
    expect(Router.getCacheControlHeader('/spa-renderer.js', 'text/javascript;charset=utf-8')).toBe('no-cache, must-revalidate');
});
test('cache policy marks Bun chunk assets immutable', () => {
    expect(Router.getCacheControlHeader('/chunk-ab12cd34.js', 'text/javascript;charset=utf-8'))
        .toBe('public, max-age=31536000, immutable');
});
test('cache policy marks dist assets cacheable', () => {
    expect(Router.getCacheControlHeader('/shared/assets/logo.svg', 'image/svg+xml'))
        .toBe('public, max-age=3600');
});
test('default CSP allows first-class Tac Wasm companions', () => {
    delete process.env.YON_CONTENT_SECURITY_POLICY;
    const headers = Router.getHeaders();
    expect(headers['Content-Security-Policy']).toContain("script-src 'self' 'wasm-unsafe-eval'");
});
test('trusted proxy support resolves forwarded client metadata', () => {
    process.env.YON_TRUST_PROXY = 'loopback';
    const request = new Request('http://127.0.0.1/api', {
        headers: {
            'x-forwarded-for': '203.0.113.10, 127.0.0.1',
            'x-forwarded-proto': 'https',
            'x-forwarded-host': 'app.example.com',
        },
    });
    expect(Yon.getClientInfo(request, '127.0.0.1')).toEqual({
        ipAddress: '203.0.113.10',
        protocol: 'https',
        host: 'app.example.com',
    });
});
test('untrusted proxy headers are ignored', () => {
    delete process.env.YON_TRUST_PROXY;
    const request = new Request('http://127.0.0.1/api', {
        headers: {
            'x-forwarded-for': '203.0.113.10',
            'x-forwarded-proto': 'https',
            'x-forwarded-host': 'app.example.com',
        },
    });
    expect(Yon.getClientInfo(request, '127.0.0.1')).toEqual({
        ipAddress: '127.0.0.1',
        protocol: 'http',
        host: '127.0.0.1',
    });
});
test('generated request IDs use FYLO TTIDs', async () => {
    const requestId = await Yon.getRequestId(new Request('http://127.0.0.1/languages/javascript'));
    const laterRequestId = await Fylo.uniqueTTID(undefined);
    expect(requestId).toMatch(/^[0-9A-Z]{11}$/);
    expect(laterRequestId).toMatch(/^[0-9A-Z]{11}$/);
    expect(laterRequestId).not.toBe(requestId);
});
test('incoming request IDs are preserved when within the supported length', async () => {
    const request = new Request('http://127.0.0.1/languages/javascript', {
        headers: { 'X-Request-Id': 'external-request-id' },
    });
    expect(await Yon.getRequestId(request)).toBe('external-request-id');
});
test('handler log paths are relative to the routes folder', () => {
    Router.routesPath = '/workspace/app/server/routes';
    expect(Yon.routeRelativeHandler('/workspace/app/server/routes/languages/javascript/GET.js'))
        .toBe('/languages/javascript/GET.js');
});
