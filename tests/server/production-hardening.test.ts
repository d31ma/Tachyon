import { afterEach, expect, test } from 'bun:test'
import Router from '../../src/server/route-handler.js'
import Tach from '../../src/server/process-executor.js'

const originalTrustProxy = process.env.TRUST_PROXY

afterEach(() => {
    if (originalTrustProxy === undefined) delete process.env.TRUST_PROXY
    else process.env.TRUST_PROXY = originalTrustProxy
})

test('cache policy marks HTML and stable runtime assets as no-cache', () => {
    expect(Router.getCacheControlHeader('/', 'text/html;charset=utf-8')).toBe('no-cache, must-revalidate')
    expect(Router.getCacheControlHeader('/main.js', 'text/javascript;charset=utf-8')).toBe('no-cache, must-revalidate')
    expect(Router.getCacheControlHeader('/spa-renderer.js', 'text/javascript;charset=utf-8')).toBe('no-cache, must-revalidate')
})

test('cache policy marks Bun chunk assets immutable', () => {
    expect(Router.getCacheControlHeader('/chunk-ab12cd34.js', 'text/javascript;charset=utf-8'))
        .toBe('public, max-age=31536000, immutable')
})

test('cache policy marks dist assets cacheable', () => {
    expect(Router.getCacheControlHeader('/assets/logo.svg', 'image/svg+xml'))
        .toBe('public, max-age=3600')
})

test('trusted proxy support resolves forwarded client metadata', () => {
    process.env.TRUST_PROXY = 'loopback'

    const request = new Request('http://127.0.0.1/api', {
        headers: {
            'x-forwarded-for': '203.0.113.10, 127.0.0.1',
            'x-forwarded-proto': 'https',
            'x-forwarded-host': 'app.example.com',
        },
    })

    expect(Tach.getClientInfo(request, '127.0.0.1')).toEqual({
        ipAddress: '203.0.113.10',
        protocol: 'https',
        host: 'app.example.com',
    })
})

test('untrusted proxy headers are ignored', () => {
    delete process.env.TRUST_PROXY

    const request = new Request('http://127.0.0.1/api', {
        headers: {
            'x-forwarded-for': '203.0.113.10',
            'x-forwarded-proto': 'https',
            'x-forwarded-host': 'app.example.com',
        },
    })

    expect(Tach.getClientInfo(request, '127.0.0.1')).toEqual({
        ipAddress: '127.0.0.1',
        protocol: 'http',
        host: '127.0.0.1',
    })
})
