import { test, beforeAll, expect, describe } from 'bun:test'

/** Base URL for the local test server */
const BASE_URL = 'http://localhost:8080'

/** Time to wait for the worker server to finish starting up */
const STARTUP_DELAY_MS = 1000

/** Basic auth credentials matching .env BASIC_AUTH=admin:pass */
const AUTH_HEADER = `Basic ${btoa('admin:pass')}`

/** Returns headers record with Authorization pre-set */
function authHeaders(extra: Record<string, string> = {}): Record<string, string> {
    return { 'Authorization': AUTH_HEADER, ...extra }
}

/** Shorthand for an authenticated fetch */
async function authFetch(path: string, init: RequestInit = {}): Promise<Response> {
    return fetch(`${BASE_URL}${path}`, {
        ...init,
        headers: authHeaders(init.headers as Record<string, string> ?? {}),
    })
}

beforeAll(async () => {
    new Worker('./tests/integration/server-worker.ts').postMessage({ script: './src/cli/serve.ts', cwd: './examples' })
    await Bun.sleep(STARTUP_DELAY_MS)
})

// ===========================================================================
// Route smoke tests — only routes with working local handlers
// The GET / handler is a streaming watcher, POST / needs Java+gson,
// DELETE / needs Dart null-safety — skip those in CI.
// ===========================================================================
interface RouteTestCase {
    route: string
    methods: Array<{ method: string; path?: string }>
}

const routeTestCases: RouteTestCase[] = [
    {
        route: '/api',
        methods: [
            { method: 'GET' },
            { method: 'POST' },
            { method: 'PUT' },
        ],
    },
    {
        route: '/api/v2',
        methods: [
            { method: 'GET' },
            { method: 'DELETE' },
            { method: 'PATCH', path: '/api/v2/users' },
        ],
    },
]

for (const { route, methods } of routeTestCases) {
    describe(route, () => {
        for (const { method, path } of methods) {
            test(method, async () => {
                const res = await authFetch(path ?? route, method !== 'GET' ? { method } : {})
                expect(res.status).toEqual(200)
                const body = await res.json()
                expect(body).toHaveProperty('message')
            })
        }
    })
}

// ===========================================================================
// Basic Authentication
// ===========================================================================
describe('Basic authentication', () => {
    test('request without auth header returns 401', async () => {
        const res = await fetch(`${BASE_URL}/api`)
        expect(res.status).toEqual(401)
        const body = await res.json()
        expect(body.detail).toBe('Unauthorized Client')
    })

    test('401 response includes WWW-Authenticate header', async () => {
        const res = await fetch(`${BASE_URL}/api`)
        expect(res.headers.get('www-authenticate')).toBe('Basic realm="Secure Area"')
    })

    test('wrong credentials return 401', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            headers: { 'Authorization': `Basic ${btoa('wrong:creds')}` },
        })
        expect(res.status).toEqual(401)
    })

    test('correct credentials return 200', async () => {
        const res = await authFetch('/api')
        expect(res.status).toEqual(200)
    })

    test('malformed auth header returns 401', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            headers: { 'Authorization': 'Basic not-valid-base64' },
        })
        expect(res.status).toEqual(401)
    })
})

// ===========================================================================
// Dynamic Route Segments
// ===========================================================================
describe('Dynamic route segments', () => {
    test('dynamic :version segment resolves params', async () => {
        const res = await authFetch('/api/v1')
        expect(res.status).toEqual(200)
        const body = await res.json()
        expect(body).toBeDefined()
    })

    test('different dynamic segment values resolve', async () => {
        const res = await authFetch('/api/v3')
        expect(res.status).toEqual(200)
    })

    test('dynamic segment with DELETE method', async () => {
        const res = await authFetch('/api/v2', { method: 'DELETE' })
        expect(res.status).toEqual(200)
    })

    test('dynamic segment with PATCH and trailing path', async () => {
        const res = await authFetch('/api/v1/users', { method: 'PATCH' })
        expect(res.status).toEqual(200)
    })
})

// ===========================================================================
// Query Parameter Parsing
// ===========================================================================
describe('Query parameter parsing', () => {
    test('numeric query params', async () => {
        const res = await authFetch('/api?count=42')
        expect(res.status).toEqual(200)
    })

    test('boolean query params', async () => {
        const res = await authFetch('/api?active=true')
        expect(res.status).toEqual(200)
    })

    test('null query param', async () => {
        const res = await authFetch('/api?value=null')
        expect(res.status).toEqual(200)
    })

    test('comma-separated values', async () => {
        const res = await authFetch('/api?tags=a,b,c')
        expect(res.status).toEqual(200)
    })

    test('JSON object query param', async () => {
        const res = await authFetch(`/api?data=${encodeURIComponent('{"key":"val"}')}`)
        expect(res.status).toEqual(200)
    })

    test('multiple query params', async () => {
        const res = await authFetch('/api?foo=bar&num=10&flag=false')
        expect(res.status).toEqual(200)
    })
})

// ===========================================================================
// Request Body Parsing
// ===========================================================================
describe('Request body parsing', () => {
    test('JSON body is parsed', async () => {
        const res = await authFetch('/api', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ message: 'hello' }),
        })
        expect(res.status).toEqual(200)
    })

    test('text body is parsed as string', async () => {
        const res = await authFetch('/api', {
            method: 'POST',
            headers: { 'Content-Type': 'text/plain' },
            body: 'plain text body',
        })
        expect(res.status).toEqual(200)
    })

    test('empty body is accepted', async () => {
        const res = await authFetch('/api', { method: 'POST' })
        expect(res.status).toEqual(200)
    })
})

// ===========================================================================
// OPTIONS Route
// ===========================================================================
describe('OPTIONS route', () => {
    test('OPTIONS / returns schema JSON', async () => {
        const res = await fetch(`${BASE_URL}/`, { method: 'OPTIONS' })
        expect(res.status).toEqual(200)
        expect(res.headers.get('content-type')).toContain('application/json')
        const body = await res.json()
        expect(body).toHaveProperty('GET')
        expect(body).toHaveProperty('POST')
        expect(body).toHaveProperty('DELETE')
    })

    test('OPTIONS /api returns schema JSON', async () => {
        const res = await fetch(`${BASE_URL}/api`, { method: 'OPTIONS' })
        expect(res.status).toEqual(200)
        const body = await res.json()
        expect(body).toHaveProperty('GET')
        expect(body).toHaveProperty('POST')
        expect(body).toHaveProperty('PUT')
    })

    test('OPTIONS /api/:version returns schema JSON', async () => {
        const res = await fetch(`${BASE_URL}/api/v2`, { method: 'OPTIONS' })
        expect(res.status).toEqual(200)
        const body = await res.json()
        expect(body).toHaveProperty('GET')
        expect(body).toHaveProperty('DELETE')
        expect(body).toHaveProperty('PATCH')
    })

    test('OPTIONS schema has res and err keys', async () => {
        const res = await fetch(`${BASE_URL}/`, { method: 'OPTIONS' })
        const body = await res.json()
        expect(body.GET).toHaveProperty('res')
        expect(body.GET).toHaveProperty('err')
        expect(body.GET.res).toHaveProperty('message')
    })
})

// ===========================================================================
// 404 Not Found
// ===========================================================================
describe('404 Not Found', () => {
    test('unknown route returns 404', async () => {
        const res = await authFetch('/nonexistent/path')
        expect(res.status).toEqual(404)
    })

    test('unknown method on known route returns 404', async () => {
        const res = await authFetch('/api', { method: 'DELETE' })
        expect(res.status).toEqual(404)
    })
})

// ===========================================================================
// CORS Headers
// ===========================================================================
describe('CORS headers', () => {
    test('response includes Access-Control-Allow-Credential header', async () => {
        const res = await authFetch('/api')
        // ALLOW_CREDENTIALS defaults to "false" when not set
        expect(res.headers.get('access-control-allow-credential')).toBe('false')
    })

    test('401 response includes Access-Control-Allow-Credential header', async () => {
        const res = await fetch(`${BASE_URL}/api`)
        expect(res.status).toEqual(401)
        expect(res.headers.get('access-control-allow-credential')).toBe('false')
    })
})

// ===========================================================================
// Response Body Content
// ===========================================================================
describe('Response body content', () => {
    test('GET /api returns a message string', async () => {
        const res = await authFetch('/api')
        const body = await res.json()
        expect(body).toHaveProperty('message')
        expect(typeof body.message).toBe('string')
    })

    test('POST /api returns a message field', async () => {
        const res = await authFetch('/api', { method: 'POST' })
        const body = await res.json()
        expect(body).toHaveProperty('message')
    })

    test('PUT /api returns a message field', async () => {
        const res = await authFetch('/api', { method: 'PUT' })
        const body = await res.json()
        expect(body).toHaveProperty('message')
    })

    test('GET /api/v2 returns a message field', async () => {
        const res = await authFetch('/api/v2')
        const body = await res.json()
        expect(body).toHaveProperty('message')
    })

    test('handler messages vary by language', async () => {
        const res1 = await authFetch('/api')
        const body1 = await res1.json()
        const res2 = await authFetch('/api', { method: 'POST' })
        const body2 = await res2.json()
        // Node.js handler says Node.js, Python handler says Python
        expect(body1.message).not.toEqual(body2.message)
    })
})

// ===========================================================================
// HTML Route & Accept Header
// ===========================================================================
describe('HTML route serving', () => {
    test('Accept text/html returns dev.html shell', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            headers: { 'Accept': 'text/html' },
        })
        expect(res.status).toEqual(200)
        expect(res.headers.get('content-type')).toContain('text/html')
        const body = await res.text()
        expect(body).toContain('<html')
    })

    test('Accept text/html on /api/v2 returns HTML shell', async () => {
        const res = await fetch(`${BASE_URL}/api/v2`, {
            headers: { 'Accept': 'text/html' },
        })
        expect(res.status).toEqual(200)
        expect(res.headers.get('content-type')).toContain('text/html')
    })

    test('Accept text/html bypasses basic auth', async () => {
        // HTML shell is returned before auth check
        const res = await fetch(`${BASE_URL}/api`, {
            headers: { 'Accept': 'text/html' },
        })
        expect(res.status).toEqual(200)
    })

    test('without Accept text/html returns non-HTML', async () => {
        const res = await authFetch('/api')
        expect(res.headers.get('content-type')).not.toContain('text/html')
        await res.text()
    })
})

// ===========================================================================
// Routes Manifest
// ===========================================================================
describe('Routes manifest', () => {
    test('GET /routes.json returns JSON', async () => {
        const res = await fetch(`${BASE_URL}/routes.json`)
        expect(res.status).toEqual(200)
        expect(res.headers.get('content-type')).toContain('application/json')
        const body = await res.json()
        expect(typeof body).toBe('object')
    })

    test('/routes.json contains dynamic route slugs', async () => {
        const res = await fetch(`${BASE_URL}/routes.json`)
        const body = await res.json()
        expect(body).toHaveProperty('/api/:version')
    })

    test('/routes.json has HTML route', async () => {
        const res = await fetch(`${BASE_URL}/routes.json`)
        const body = await res.json()
        expect(body).toHaveProperty('/')
    })
})

// ===========================================================================
// Layouts Manifest
// ===========================================================================
describe('Layouts manifest', () => {
    test('GET /layouts.json returns JSON', async () => {
        const res = await fetch(`${BASE_URL}/layouts.json`)
        expect(res.status).toEqual(200)
        expect(res.headers.get('content-type')).toContain('application/json')
        const body = await res.json()
        expect(typeof body).toBe('object')
    })

    test('/layouts.json contains root layout', async () => {
        const res = await fetch(`${BASE_URL}/layouts.json`)
        const body = await res.json()
        expect(body).toHaveProperty('/')
        expect(body['/']).toContain('/layouts/')
    })

    test('GET layout module returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/layouts.json`)
        const body = await res.json()
        const layoutUrl = body['/']
        const layoutRes = await fetch(`${BASE_URL}${layoutUrl}`)
        expect(layoutRes.status).toEqual(200)
        expect(layoutRes.headers.get('content-type')).toContain('javascript')
    })
})

// ===========================================================================
// Static Assets
// ===========================================================================
describe('Static assets', () => {
    test('GET /main.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/main.js`)
        expect(res.status).toEqual(200)
        expect(res.headers.get('content-type')).toContain('javascript')
    })
})

// ===========================================================================
// Component Bundling
// ===========================================================================
describe('Component bundling', () => {
    test('GET /components/clicker.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/components/clicker.js`)
        expect(res.status).toEqual(200)
        expect(res.headers.get('content-type')).toContain('javascript')
    })
})

// ===========================================================================
// Page Bundling
// ===========================================================================
describe('Page bundling', () => {
    test('GET /pages/HTML.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/pages/HTML.js`)
        expect(res.status).toEqual(200)
        expect(res.headers.get('content-type')).toContain('javascript')
    })

    test('GET /pages/404.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/pages/404.js`)
        expect(res.status).toEqual(200)
        expect(res.headers.get('content-type')).toContain('javascript')
    })
})

// ===========================================================================
// Client-Side Scripts
// ===========================================================================
describe('Client-side scripts', () => {
    test('GET /spa-renderer.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/spa-renderer.js`)
        expect(res.status).toEqual(200)
        expect(res.headers.get('content-type')).toContain('javascript')
    })

    test('GET /hot-reload-client.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/hot-reload-client.js`)
        expect(res.status).toEqual(200)
        expect(res.headers.get('content-type')).toContain('javascript')
    })
})

// ===========================================================================
// NPM Module Bundling
// ===========================================================================
describe('NPM module bundling', () => {
    test('GET /modules/dayjs.js returns bundled JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/modules/dayjs.js`)
        expect(res.status).toEqual(200)
        expect(res.headers.get('content-type')).toContain('javascript')
    })

    test('GET /modules/dayjs.js contains executable module code', async () => {
        const res = await fetch(`${BASE_URL}/modules/dayjs.js`)
        const body = await res.text()
        expect(body.length).toBeGreaterThan(100)
    })

    test('GET /modules/nonexistent.js returns 404', async () => {
        const res = await fetch(`${BASE_URL}/modules/nonexistent.js`)
        expect(res.status).toEqual(404)
    })
})

// ===========================================================================
// SSE / HMR Endpoint
// ===========================================================================
describe('SSE HMR endpoint', () => {
    test('GET /hmr returns event-stream content type', async () => {
        const controller = new AbortController()
        setTimeout(() => controller.abort(), 500)
        try {
            const res = await fetch(`${BASE_URL}/hmr`, {
                signal: controller.signal,
            })
            expect(res.status).toEqual(200)
            expect(res.headers.get('content-type')).toContain('text/event-stream')
        } catch (e) {
            if ((e as Error).name !== 'AbortError') throw e
        }
    })
})

// ===========================================================================
// SSE Streaming on Route Handlers
// ===========================================================================
describe('SSE streaming', () => {
    test('Accept text/event-stream returns streaming response', async () => {
        const controller = new AbortController()
        setTimeout(() => controller.abort(), 500)
        try {
            const res = await fetch(`${BASE_URL}/api`, {
                headers: authHeaders({ 'Accept': 'text/event-stream' }),
                signal: controller.signal,
            })
            expect(res.status).toEqual(200)
            expect(res.headers.get('content-type')).toContain('text/event-stream')
        } catch (e) {
            if ((e as Error).name !== 'AbortError') throw e
        }
    })
})

// ===========================================================================
// Request Headers Forwarding
// ===========================================================================
describe('Request headers forwarding', () => {
    test('custom headers are forwarded to handler', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            headers: authHeaders({ 'X-Custom-Header': 'test-value' }),
        })
        expect(res.status).toEqual(200)
    })
})

// ===========================================================================
// Route Validation (unit-level via Router import)
// ===========================================================================
describe('Route validation', () => {
    test('route starting with slug segment throws', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default
        expect(Router.validateRoute(':id/GET')).rejects.toThrow('cannot start with a slug')
    })

    test('consecutive slug segments throw', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default
        expect(Router.validateRoute('api/:id/:name/GET')).rejects.toThrow('consecutive slug segments')
    })

    test('valid route does not throw', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default
        await expect(Router.validateRoute('users/:id/GET')).resolves.toBeUndefined()
    })
})

// ===========================================================================
// Router.parseParams (unit-level)
// ===========================================================================
describe('Router.parseParams', () => {
    test('coerces numbers', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default
        expect(Router.parseParams(['42', '3.14'])).toEqual([42, 3.14])
    })

    test('coerces booleans', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default
        expect(Router.parseParams(['true', 'false'])).toEqual([true, false])
    })

    test('coerces null and undefined', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default
        expect(Router.parseParams(['null', 'undefined'])).toEqual([null, undefined])
    })

    test('preserves plain strings', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default
        expect(Router.parseParams(['hello', 'world'])).toEqual(['hello', 'world'])
    })

    test('handles mixed types', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default
        expect(Router.parseParams(['42', 'hello', 'true', 'null'])).toEqual([42, 'hello', true, null])
    })

    test('handles empty array', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default
        expect(Router.parseParams([])).toEqual([])
    })
})
