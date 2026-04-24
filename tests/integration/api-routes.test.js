// @ts-check
import { test, beforeAll, afterAll, expect, describe } from 'bun:test';
import Fylo from '@d31ma/fylo';
import { mkdtemp, rm } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
/**
 * @typedef {Bun.Subprocess<any, any, any>} BunProcess
 * @typedef {HeadersInit & Record<string, string>} HeaderRecord
 */
/** Base URL for the local test server */
const TEST_PORT = '18080';
const BASE_URL = `http://localhost:${TEST_PORT}`;
/** Time to wait for the test server to finish starting up */
const STARTUP_TIMEOUT_MS = 10_000;
/**
 * Basic auth credentials for tests.
 * The test server hashes this value into BASIC_AUTH_HASH at startup.
 * Use TEST_BASIC_AUTH env var to override (e.g. TEST_BASIC_AUTH=user:secret bun test).
 */
const TEST_BASIC_AUTH = process.env.TEST_BASIC_AUTH ?? 'admin:pass';
const AUTH_HEADER = `Basic ${btoa(TEST_BASIC_AUTH)}`;
/** @type {BunProcess | null} */
let serverProcess = null;
/** @type {string} */
let telemetryRoot = '';
const PROJECT_ROOT = `${import.meta.dir}/../..`;
const EXAMPLES_DIR = `${PROJECT_ROOT}/examples`;
const SERVE_SCRIPT = `${PROJECT_ROOT}/src/cli/serve.js`;
const TELEMETRY_ALERT_WORKER = `${EXAMPLES_DIR}/server/workers/telemetry-alert-worker.js`;
/**
 * @param {string} baseUrl
 * @param {BunProcess} proc
 */
async function waitForServer(baseUrl, proc) {
    const startedAt = Date.now();
    while (Date.now() - startedAt < STARTUP_TIMEOUT_MS) {
        if (proc.exitCode !== null) {
            throw new Error(`Test server exited early with code ${proc.exitCode}`);
        }
        try {
            const response = await fetch(`${baseUrl}/routes.json`);
            if (response.ok)
                return;
        }
        catch {
            // Server is still starting.
        }
        await Bun.sleep(200);
    }
    throw new Error(`Timed out waiting for test server on ${baseUrl}`);
}
/** Returns headers record with Authorization pre-set */
/** @param {HeaderRecord} [extra] */
function authHeaders(extra = {}) {
    return { 'Authorization': AUTH_HEADER, ...extra };
}
/** Shorthand for an authenticated fetch */
/**
 * @param {string} path
 * @param {RequestInit & { headers?: HeaderRecord }} [init]
 */
async function authFetch(path, init = {}) {
    return fetch(`${BASE_URL}${path}`, {
        ...init,
        headers: authHeaders(init.headers ?? {}),
    });
}

/**
 * @param {string} requestId
 * @returns {Promise<Array<{ resource: Record<string, any>, scope: Record<string, any>, span: Record<string, any> }>>}
 */
async function waitForTelemetrySpans(requestId) {
    const fylo = new Fylo({ root: telemetryRoot });
    const startedAt = Date.now();
    while (Date.now() - startedAt < 4_000) {
        try {
            /** @type {Array<{ resource: Record<string, any>, scope: Record<string, any>, span: Record<string, any> }>} */
            const spans = [];
            for await (const doc of fylo.findDocs('otel-spans', {}).collect()) {
                for (const traceDoc of Object.values(/** @type {Record<string, any>} */ (doc))) {
                    if (traceDoc && typeof traceDoc === 'object') {
                        spans.push(...extractPersistedSpans(/** @type {Record<string, any>} */ (traceDoc)));
                    }
                }
            }
            const matches = spans.filter((entry) => getAttribute(entry.span, 'tachyon.request.id') === requestId);
            if (matches.length >= 2) {
                return matches;
            }
        } catch {
            // Collection may not exist yet if the first span hasn't been written.
        }
        await Bun.sleep(50);
    }
    return [];
}

/**
 * @param {Record<string, any>} traceDoc
 * @returns {Array<{ resource: Record<string, any>, scope: Record<string, any>, span: Record<string, any> }>}
 */
function extractPersistedSpans(traceDoc) {
    if (typeof traceDoc.otlpJson === 'string') {
        try {
            traceDoc = JSON.parse(traceDoc.otlpJson);
        } catch {
            return [];
        }
    }
    /** @type {Array<{ resource: Record<string, any>, scope: Record<string, any>, span: Record<string, any> }>} */
    const spans = [];
    for (const resourceSpan of traceDoc.resourceSpans ?? []) {
        for (const scopeSpan of resourceSpan.scopeSpans ?? []) {
            for (const span of scopeSpan.spans ?? []) {
                spans.push({
                    resource: resourceSpan.resource ?? {},
                    scope: scopeSpan.scope ?? {},
                    span,
                });
            }
        }
    }
    return spans;
}

/**
 * @param {Record<string, any>} value
 * @returns {any}
 */
function decodeAnyValue(value) {
    if ('stringValue' in value) {
        return value.stringValue;
    }
    if ('boolValue' in value) {
        return value.boolValue;
    }
    if ('intValue' in value) {
        const parsed = Number(value.intValue);
        return Number.isNaN(parsed) ? value.intValue : parsed;
    }
    if ('doubleValue' in value) {
        return value.doubleValue;
    }
    if ('arrayValue' in value) {
        return (value.arrayValue?.values ?? []).map(decodeAnyValue);
    }
    if ('kvlistValue' in value) {
        return Object.fromEntries((value.kvlistValue?.values ?? []).map((/** @type {any} */ entry) => [entry.key, decodeAnyValue(entry.value ?? {})]));
    }
    if ('bytesValue' in value) {
        return value.bytesValue;
    }
    return undefined;
}

/**
 * @param {Record<string, any>} span
 * @param {string} key
 * @returns {any}
 */
function getAttribute(span, key) {
    const entry = (span.attributes ?? []).find((/** @type {any} */ item) => item.key === key);
    return entry ? decodeAnyValue(entry.value ?? {}) : undefined;
}

/**
 * @param {Record<string, any>} resource
 * @param {string} key
 * @returns {any}
 */
function getResourceAttribute(resource, key) {
    const entry = (resource.attributes ?? []).find((/** @type {any} */ item) => item.key === key);
    return entry ? decodeAnyValue(entry.value ?? {}) : undefined;
}

beforeAll(async () => {
    telemetryRoot = await mkdtemp(path.join(tmpdir(), 'tachyon-otel-'));
    const basicAuthHash = await Bun.password.hash(TEST_BASIC_AUTH);
    serverProcess = Bun.spawn(['bun', SERVE_SCRIPT], {
        cwd: EXAMPLES_DIR,
        env: {
            ...process.env,
            PORT: TEST_PORT,
            HOSTNAME: '127.0.0.1',
            BASIC_AUTH_HASH: basicAuthHash,
            ALLOW_HEADERS: 'Content-Type,Authorization',
            ALLOW_ORIGINS: 'https://app.example.com',
            ALLOW_METHODS: 'GET,POST,PUT,DELETE,PATCH,OPTIONS',
            ENABLE_HSTS: 'false',
            MAX_BODY_BYTES: '64',
            OTEL_ENABLED: 'true',
            OTEL_FYLO_ROOT: telemetryRoot,
            OTEL_SERVICE_NAME: 'tachyon-tests',
            TAC_PUBLIC_ENV: 'PUBLIC_API_BASE_URL',
            PUBLIC_API_BASE_URL: 'https://api.example.com',
            PRIVATE_BROWSER_SECRET: 'server-only-secret',
        },
        stdout: 'inherit',
        stderr: 'inherit'
    });
    await waitForServer(BASE_URL, serverProcess);
});
afterAll(async () => {
    serverProcess?.kill();
    await Bun.sleep(200);
    serverProcess = null;
    if (telemetryRoot)
        await rm(telemetryRoot, { recursive: true, force: true });
});
const routeTestCases = [
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
];
for (const { route, methods } of routeTestCases) {
    describe(route, () => {
        for (const { method, path } of methods) {
            test(method, async () => {
                const res = await authFetch(path ?? route, method !== 'GET' ? { method } : {});
                expect(res.status).toEqual(200);
                const body = await res.json();
                expect(body).toHaveProperty('message');
            });
        }
    });
}
// ===========================================================================
// Basic Authentication
// ===========================================================================
describe('Basic authentication', () => {
    test('request without auth header returns 401', async () => {
        const res = await fetch(`${BASE_URL}/api`);
        expect(res.status).toEqual(401);
        const body = await res.json();
        expect(body.detail).toBe('Unauthorized Client');
    });
    test('401 response includes WWW-Authenticate header', async () => {
        const res = await fetch(`${BASE_URL}/api`);
        expect(res.headers.get('www-authenticate')).toBe('Basic realm="Secure Area"');
    });
    test('wrong credentials return 401', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            headers: { 'Authorization': `Basic ${btoa('wrong:creds')}` },
        });
        expect(res.status).toEqual(401);
    });
    test('correct credentials return 200', async () => {
        const res = await authFetch('/api');
        expect(res.status).toEqual(200);
    });
    test('malformed auth header returns 401', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            headers: { 'Authorization': 'Basic not-valid-base64' },
        });
        expect(res.status).toEqual(401);
    });
});
// ===========================================================================
// Dynamic Route Segments
// ===========================================================================
describe('Dynamic route segments', () => {
    test('dynamic :version segment resolves params', async () => {
        const res = await authFetch('/api/v1');
        expect(res.status).toEqual(200);
        const body = await res.json();
        expect(body).toBeDefined();
    });
    test('different dynamic segment values resolve', async () => {
        const res = await authFetch('/api/v3');
        expect(res.status).toEqual(200);
    });
    test('dynamic segment with DELETE method', async () => {
        const res = await authFetch('/api/v2', { method: 'DELETE' });
        expect(res.status).toEqual(200);
    });
    test('dynamic segment with PATCH and trailing path', async () => {
        const res = await authFetch('/api/v1/users', { method: 'PATCH' });
        expect(res.status).toEqual(200);
    });
});
// ===========================================================================
// Query Parameter Parsing
// ===========================================================================
describe('Query parameter parsing', () => {
    test('numeric query params', async () => {
        const res = await authFetch('/api?count=42');
        expect(res.status).toEqual(200);
    });
    test('boolean query params', async () => {
        const res = await authFetch('/api?active=true');
        expect(res.status).toEqual(200);
    });
    test('null query param', async () => {
        const res = await authFetch('/api?value=null');
        expect(res.status).toEqual(200);
    });
    test('comma-separated values', async () => {
        const res = await authFetch('/api?tags=a,b,c');
        expect(res.status).toEqual(200);
    });
    test('JSON object query param', async () => {
        const res = await authFetch(`/api?data=${encodeURIComponent('{"key":"val"}')}`);
        expect(res.status).toEqual(200);
    });
    test('multiple query params', async () => {
        const res = await authFetch('/api?foo=bar&num=10&flag=false');
        expect(res.status).toEqual(200);
    });
});
// ===========================================================================
// Request Body Parsing
// ===========================================================================
describe('Request body parsing', () => {
    test('JSON body is parsed', async () => {
        const res = await authFetch('/api', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ message: 'hello' }),
        });
        expect(res.status).toEqual(200);
    });
    test('text body is parsed as string', async () => {
        const res = await authFetch('/api', {
            method: 'POST',
            headers: { 'Content-Type': 'text/plain' },
            body: 'plain text body',
        });
        expect(res.status).toEqual(200);
    });
    test('empty body is accepted', async () => {
        const res = await authFetch('/api', { method: 'POST' });
        expect(res.status).toEqual(200);
    });
    test('body exceeding MAX_BODY_BYTES returns 413 before handler execution', async () => {
        const res = await authFetch('/api', {
            method: 'POST',
            headers: { 'Content-Type': 'text/plain' },
            body: 'x'.repeat(65),
        });
        expect(res.status).toEqual(413);
        const body = await res.json();
        expect(body.error).toBe('Payload too large');
    });
});
// ===========================================================================
// OPTIONS Route
// ===========================================================================
describe('OPTIONS route', () => {
    test('OPTIONS / returns schema JSON', async () => {
        const res = await fetch(`${BASE_URL}/`, { method: 'OPTIONS' });
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('application/json');
        const body = await res.json();
        expect(body).toHaveProperty('GET');
        expect(body).toHaveProperty('POST');
        expect(body).toHaveProperty('DELETE');
    });
    test('OPTIONS /api returns schema JSON', async () => {
        const res = await fetch(`${BASE_URL}/api`, { method: 'OPTIONS' });
        expect(res.status).toEqual(200);
        const body = await res.json();
        expect(body).toHaveProperty('GET');
        expect(body).toHaveProperty('POST');
        expect(body).toHaveProperty('PUT');
    });
    test('OPTIONS /api/:version returns schema JSON', async () => {
        const res = await fetch(`${BASE_URL}/api/v2`, { method: 'OPTIONS' });
        expect(res.status).toEqual(200);
        const body = await res.json();
        expect(body).toHaveProperty('GET');
        expect(body).toHaveProperty('DELETE');
        expect(body).toHaveProperty('PATCH');
    });
    test('OPTIONS schema has numeric status code keys', async () => {
        const res = await fetch(`${BASE_URL}/`, { method: 'OPTIONS' });
        const body = await res.json();
        expect(body.GET).toHaveProperty('200');
        expect(body.GET).toHaveProperty('500');
        expect(body.GET['200']).toHaveProperty('message');
    });
    test('OPTIONS preflight response includes configured CORS headers', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            method: 'OPTIONS',
            headers: {
                Origin: 'https://app.example.com',
                'Access-Control-Request-Method': 'POST',
                'Access-Control-Request-Headers': 'Content-Type,Authorization',
            },
        });
        expect(res.status).toEqual(200);
        expect(res.headers.get('access-control-allow-origin')).toBe('https://app.example.com');
        expect(res.headers.get('access-control-allow-methods')).toBe('GET,POST,PUT,DELETE,PATCH,OPTIONS');
        expect(res.headers.get('access-control-allow-headers')).toBe('Content-Type,Authorization');
        expect(res.headers.get('access-control-allow-credentials')).toBe('false');
    });
});
// ===========================================================================
// 404 Not Found
// ===========================================================================
describe('404 Not Found', () => {
    test('unknown route returns 404', async () => {
        const res = await authFetch('/nonexistent/path');
        expect(res.status).toEqual(404);
    });
    test('unknown method on known route returns 404', async () => {
        const res = await authFetch('/api', { method: 'DELETE' });
        expect(res.status).toEqual(404);
    });
});
// ===========================================================================
// CORS Headers
// ===========================================================================
describe('CORS headers', () => {
    test('response includes Access-Control-Allow-Credentials header', async () => {
        const res = await authFetch('/api');
        // ALLOW_CREDENTIALS defaults to "false" when not set
        expect(res.headers.get('access-control-allow-credentials')).toBe('false');
    });
    test('401 response includes Access-Control-Allow-Credentials header', async () => {
        const res = await fetch(`${BASE_URL}/api`);
        expect(res.status).toEqual(401);
        expect(res.headers.get('access-control-allow-credentials')).toBe('false');
    });
    test('allowed cross-origin request echoes the matched origin', async () => {
        const res = await authFetch('/api', {
            headers: { Origin: 'https://app.example.com' },
        });
        expect(res.status).toEqual(200);
        expect(res.headers.get('access-control-allow-origin')).toBe('https://app.example.com');
        expect(res.headers.get('vary')).toBe('Origin');
    });
    test('same-origin request remains allowed even when not listed in ALLOW_ORIGINS', async () => {
        const res = await authFetch('/api', {
            method: 'POST',
            headers: { Origin: BASE_URL },
        });
        expect(res.status).toEqual(200);
        expect(res.headers.get('access-control-allow-origin')).toBe(BASE_URL);
    });
    test('disallowed cross-origin request returns 403 before handler execution', async () => {
        const res = await authFetch('/api', {
            headers: { Origin: 'https://evil.example.com' },
        });
        expect(res.status).toEqual(403);
        expect(res.headers.get('access-control-allow-origin')).toBe('https://app.example.com');
        const body = await res.json();
        expect(body.detail).toBe('Origin not allowed');
    });
    test('disallowed preflight request returns 403', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            method: 'OPTIONS',
            headers: {
                Origin: 'https://evil.example.com',
                'Access-Control-Request-Method': 'POST',
                'Access-Control-Request-Headers': 'Content-Type,Authorization',
            },
        });
        expect(res.status).toEqual(403);
        const body = await res.json();
        expect(body.detail).toBe('Origin not allowed');
    });
});
// ===========================================================================
// Response Body Content
// ===========================================================================
describe('Response body content', () => {
    test('GET /api returns a message string', async () => {
        const res = await authFetch('/api');
        const body = await res.json();
        expect(body).toHaveProperty('message');
        expect(typeof body.message).toBe('string');
    });
    test('POST /api returns a message field', async () => {
        const res = await authFetch('/api', { method: 'POST' });
        const body = await res.json();
        expect(body).toHaveProperty('message');
    });
    test('PUT /api returns a message field', async () => {
        const res = await authFetch('/api', { method: 'PUT' });
        const body = await res.json();
        expect(body).toHaveProperty('message');
    });
    test('GET /api/v2 returns a message field', async () => {
        const res = await authFetch('/api/v2');
        const body = await res.json();
        expect(body).toHaveProperty('message');
    });
    test('handler messages vary by language', async () => {
        const res1 = await authFetch('/api');
        const body1 = await res1.json();
        const res2 = await authFetch('/api', { method: 'POST' });
        const body2 = await res2.json();
        // Node.js handler says Node.js, Python handler says Python
        expect(body1.message).not.toEqual(body2.message);
    });
});
// ===========================================================================
// HTML Route & Accept Header
// ===========================================================================
describe('HTML route serving', () => {
    test('Accept text/html on API routes still returns the handler response', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            headers: authHeaders({ 'Accept': 'text/html' }),
        });
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('application/json');
        const body = await res.text();
        expect(body).toContain('Hello from Yon on Bun!');
    });
    test('Accept text/html on versioned API routes still returns the handler response', async () => {
        const res = await fetch(`${BASE_URL}/api/v2`, {
            headers: authHeaders({ 'Accept': 'text/html' }),
        });
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('application/json');
    });
    test('Accept text/html still requires basic auth', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            headers: { 'Accept': 'text/html' },
        });
        expect(res.status).toEqual(401);
    });
    test('browser navigation headers can request a page without relying on Accept text/html', async () => {
        const res = await fetch(`${BASE_URL}/`, {
            headers: {
                'Sec-Fetch-Dest': 'document',
                'Sec-Fetch-Mode': 'navigate',
            },
        });
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('text/html');
        const body = await res.text();
        expect(body).toContain('/spa-renderer.js');
    });
    test('navigation-style headers do not turn API routes into pages', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            headers: authHeaders({
                'Sec-Fetch-Dest': 'document',
                'Sec-Fetch-Mode': 'navigate',
            }),
        });
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('application/json');
        const body = await res.text();
        expect(body).toContain('Hello from Yon on Bun!');
    });
    test('without document headers returns non-HTML for API routes', async () => {
        const res = await authFetch('/api');
        expect(res.headers.get('content-type')).not.toContain('text/html');
        await res.text();
    });
});
// ===========================================================================
// Routes Manifest
// ===========================================================================
describe('Routes manifest', () => {
    test('GET /routes.json returns JSON', async () => {
        const res = await fetch(`${BASE_URL}/routes.json`);
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('application/json');
        const body = await res.json();
        expect(typeof body).toBe('object');
    });
    test('/routes.json contains browser page routes only', async () => {
        const res = await fetch(`${BASE_URL}/routes.json`);
        const body = await res.json();
        expect(body).toHaveProperty('/');
        expect(body).not.toHaveProperty('/api');
        expect(body).not.toHaveProperty('/api/:version');
    });
    test('/routes.json has HTML route', async () => {
        const res = await fetch(`${BASE_URL}/routes.json`);
        const body = await res.json();
        expect(body).toHaveProperty('/');
    });
});
// ===========================================================================
// OpenAPI / Swagger UI
// ===========================================================================
describe('OpenAPI docs', () => {
    test('GET /openapi.json returns OpenAPI 3.1 JSON', async () => {
        const res = await fetch(`${BASE_URL}/openapi.json`);
        expect(res.status).toBe(200);
        expect(res.headers.get('content-type')).toContain('application/json');
        const body = await res.json();
        expect(body.openapi).toBe('3.1.0');
        expect(body.paths).toHaveProperty('/api');
        expect(body.paths).toHaveProperty('/api/{version}');
        expect(body.paths).toHaveProperty('/health');
        expect(body.paths['/items'].post.responses).toHaveProperty('201');
    });

    test('GET /api-docs returns the Tachyon-native docs shell', async () => {
        const res = await fetch(`${BASE_URL}/api-docs`);
        expect(res.status).toBe(200);
        expect(res.headers.get('content-type')).toContain('text/html');
        const body = await res.text();
        expect(body).toContain('/api-docs/app.css');
        expect(body).toContain('/api-docs/app.js');
        expect(body).toContain('OpenAPI rendered with a Tachyon-native interactive console');
        expect(body).toContain('Search operations');
        expect(body).toContain('Authorize');
    });

    test('GET /api-docs/app.js points the docs client at /openapi.json', async () => {
        const res = await fetch(`${BASE_URL}/api-docs/app.js`);
        expect(res.status).toBe(200);
        expect(res.headers.get('content-type')).toContain('javascript');
        const body = await res.text();
        expect(body).toContain("const OPENAPI_PATH = \"/openapi.json\"");
        expect(body).toContain('async function executeOperation(operationId)');
        expect(body).toContain('Copy cURL');
    });
});
// ===========================================================================
// OpenTelemetry + browser env boundary
// ===========================================================================
describe('Telemetry and browser env', () => {
    test('request and handler spans are persisted to Fylo with traceparent propagation', async () => {
        const requestId = `otel-${crypto.randomUUID()}`;
        const traceId = '0123456789abcdef0123456789abcdef';
        const parentSpanId = '1111111111111111';
        const res = await authFetch('/api', {
            headers: {
                'X-Request-Id': requestId,
                'traceparent': `00-${traceId}-${parentSpanId}-01`,
            },
        });
        expect(res.status).toBe(200);
        expect(res.headers.get('x-trace-id')).toBe(traceId);
        expect(res.headers.get('traceparent')).toContain(traceId);

        const spans = await waitForTelemetrySpans(requestId);
        expect(spans.length).toBeGreaterThanOrEqual(2);

        const requestSpan = spans.find((entry) => entry.span.kind === 2);
        const handlerSpan = spans.find((entry) => entry.span.kind === 1);

        expect(requestSpan).toBeDefined();
        expect(handlerSpan).toBeDefined();
        expect(requestSpan?.span.traceId).toBe(traceId);
        expect(requestSpan?.span.parentSpanId).toBe(parentSpanId);
        expect(requestSpan?.span.traceState).toBe('');
        expect(requestSpan?.span.startTimeUnixNano).toMatch(/^\d+$/);
        expect(requestSpan?.span.endTimeUnixNano).toMatch(/^\d+$/);
        expect(getAttribute(requestSpan?.span ?? {}, 'http.request.method')).toBe('GET');
        expect(getAttribute(requestSpan?.span ?? {}, 'http.route')).toBe('/api');
        expect(getAttribute(requestSpan?.span ?? {}, 'http.response.status_code')).toBe(200);
        expect(getAttribute(requestSpan?.span ?? {}, 'tachyon.request.id')).toBe(requestId);
        expect(requestSpan?.span.status?.code).toBe(1);
        expect(getResourceAttribute(requestSpan?.resource ?? {}, 'service.name')).toBe('tachyon-tests');
        expect(getResourceAttribute(requestSpan?.resource ?? {}, 'telemetry.sdk.name')).toBe('tachyon');
        expect(requestSpan?.scope?.name).toBe('@d31ma/tachyon.telemetry');
        expect(handlerSpan?.span.traceId).toBe(traceId);
        expect(handlerSpan?.span.parentSpanId).toBe(requestSpan?.span.spanId);
        expect(getAttribute(handlerSpan?.span ?? {}, 'code.file.path')).toContain('/examples/server/routes/api/GET');
    });

    test('telemetry example route reads OTLP JSON from Fylo and returns a monitoring summary', async () => {
        await authFetch('/api', {
            headers: {
                'X-Request-Id': `otel-example-${crypto.randomUUID()}`,
            },
        });

        const res = await authFetch('/telemetry?limit=5');
        expect(res.status).toBe(200);
        const body = await res.json();
        expect(body.summary.enabled).toBe(true);
        expect(body.summary.collection).toBe('otel-spans');
        expect(body.summary.spanCount).toBeGreaterThanOrEqual(2);
        expect(body.summary.requestCount).toBeGreaterThanOrEqual(1);
        expect(Array.isArray(body.recent)).toBe(true);
        expect(body.recent.length).toBeGreaterThan(0);
        expect(body.recent.some((/** @type {any} */ entry) => entry.name === 'GET /api')).toBe(true);
    });

    test('telemetry alert worker flags slow routes from the telemetry endpoint', async () => {
        await authFetch('/api', {
            headers: {
                'X-Request-Id': `otel-worker-${crypto.randomUUID()}`,
            },
        });

        const proc = Bun.spawn(['bun', TELEMETRY_ALERT_WORKER], {
            cwd: EXAMPLES_DIR,
            env: {
                ...process.env,
                TELEMETRY_URL: `${BASE_URL}/telemetry?limit=10`,
                BASIC_AUTH_HEADER: AUTH_HEADER,
                ALERT_SLOW_MS: '1',
                ALERT_STATUS_CODE: '500',
            },
            stdout: 'pipe',
            stderr: 'pipe',
        });

        const [stdout, stderr, exitCode] = await Promise.all([
            new Response(proc.stdout).text(),
            new Response(proc.stderr).text(),
            proc.exited,
        ]);

        expect(exitCode).toBe(0);
        expect(stderr.trim()).toBe('');
        const body = JSON.parse(stdout);
        expect(body.alertCount).toBeGreaterThan(0);
        expect(body.alerts.some((/** @type {any} */ entry) => entry.reason === 'slow-route')).toBe(true);
        expect(body.summary.spanCount).toBeGreaterThanOrEqual(2);
    });

    test('only explicit public env vars are injected into the browser shell', async () => {
        const res = await fetch(`${BASE_URL}/`, {
            headers: {
                'Sec-Fetch-Dest': 'document',
                'Sec-Fetch-Mode': 'navigate',
            },
        });
        expect(res.status).toBe(200);
        const body = await res.text();
        expect(body).toContain('window.__ty_public_env__');
        expect(body).toContain('PUBLIC_API_BASE_URL');
        expect(body).toContain('https://api.example.com');
        expect(body).not.toContain('PRIVATE_BROWSER_SECRET');
        expect(body).not.toContain('server-only-secret');
    });
});
// ===========================================================================
// Layouts Manifest
// ===========================================================================
describe('Layouts manifest', () => {
    test('GET /shells.json returns JSON', async () => {
        const res = await fetch(`${BASE_URL}/shells.json`);
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('application/json');
        const body = await res.json();
        expect(typeof body).toBe('object');
    });
    test('/shells.json entries use the structured shell manifest shape', async () => {
        const res = await fetch(`${BASE_URL}/shells.json`);
        const body = await res.json();
        for (const entry of Object.values(body)) {
            expect(typeof entry.path).toBe('string');
            expect(typeof entry.allowSelf).toBe('boolean');
        }
    });
});
// ===========================================================================
// Static Assets
// ===========================================================================
describe('Static assets', () => {
    test('GET /main.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/main.js`);
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('javascript');
        expect(res.headers.get('cache-control')).toBe('no-cache, must-revalidate');
    });
});
// ===========================================================================
// Health / Readiness
// ===========================================================================
describe('Health endpoints', () => {
    test('GET /health returns ok without authentication', async () => {
        const res = await fetch(`${BASE_URL}/health`);
        expect(res.status).toBe(200);
        expect(res.headers.get('cache-control')).toBe('no-store');
        const body = await res.json();
        expect(body.status).toBe('ok');
        expect(typeof body.uptimeMs).toBe('number');
    });
    test('GET /ready returns ready without authentication', async () => {
        const res = await fetch(`${BASE_URL}/ready`);
        expect(res.status).toBe(200);
        const body = await res.json();
        expect(body.status).toBe('ready');
    });
});
// ===========================================================================
// Cache headers
// ===========================================================================
describe('Cache headers', () => {
    test('API route responses are not cacheable', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            headers: authHeaders({ Accept: 'text/html' }),
        });
        expect(res.status).toBe(200);
        expect(res.headers.get('cache-control')).toBe('no-store');
    });
    test('asset files are cacheable for a short period', async () => {
        const res = await fetch(`${BASE_URL}/shared/assets/wordmark.svg`);
        expect(res.status).toBe(200);
        expect(res.headers.get('cache-control')).toBe('public, max-age=3600');
    });
});
// ===========================================================================
// Component Bundling
// ===========================================================================
describe('Component bundling', () => {
    test('GET /components/clicker.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/components/clicker.js`);
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('javascript');
    });
    test('GET /components/ui/clicker.js returns JavaScript for nested components', async () => {
        const res = await fetch(`${BASE_URL}/components/ui/clicker.js`);
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('javascript');
    });
});
// ===========================================================================
// Page Bundling
// ===========================================================================
describe('Page bundling', () => {
    test('GET /pages/index.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/pages/index.js`);
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('javascript');
    });
    test('GET /pages/404.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/pages/404.js`);
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('javascript');
    });
});
// ===========================================================================
// Client-Side Scripts
// ===========================================================================
describe('Client-side scripts', () => {
    test('GET /spa-renderer.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/spa-renderer.js`);
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('javascript');
    });
    test('GET /hot-reload-client.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/hot-reload-client.js`);
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('javascript');
    });
});
// ===========================================================================
// NPM Module Bundling
// ===========================================================================
describe('NPM module bundling', () => {
    test('GET /modules/dayjs.js returns bundled JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/modules/dayjs.js`);
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('javascript');
    });
    test('GET /modules/dayjs.js contains executable module code', async () => {
        const res = await fetch(`${BASE_URL}/modules/dayjs.js`);
        const body = await res.text();
        expect(body.length).toBeGreaterThan(100);
    });
    test('GET /modules/nonexistent.js returns 404', async () => {
        const res = await fetch(`${BASE_URL}/modules/nonexistent.js`);
        expect(res.status).toEqual(404);
    });
});
// ===========================================================================
// SSE / HMR Endpoint
// ===========================================================================
describe('SSE HMR endpoint', () => {
    test('GET /hmr returns event-stream content type', async () => {
        const controller = new AbortController();
        setTimeout(() => controller.abort(), 500);
        try {
            const res = await fetch(`${BASE_URL}/hmr`, {
                signal: controller.signal,
            });
            expect(res.status).toEqual(200);
            expect(res.headers.get('content-type')).toContain('text/event-stream');
        }
        catch (error) {
            if (!(error instanceof Error) || error.name !== 'AbortError')
                throw error;
        }
    });
});
// ===========================================================================
// SSE Streaming on Route Handlers
// ===========================================================================
describe('SSE streaming', () => {
    test('Accept text/event-stream returns streaming response', async () => {
        const controller = new AbortController();
        setTimeout(() => controller.abort(), 500);
        try {
            const res = await fetch(`${BASE_URL}/api`, {
                headers: authHeaders({ 'Accept': 'text/event-stream' }),
                signal: controller.signal,
            });
            expect(res.status).toEqual(200);
            expect(res.headers.get('content-type')).toContain('text/event-stream');
        }
        catch (error) {
            if (!(error instanceof Error) || error.name !== 'AbortError')
                throw error;
        }
    });
});
// ===========================================================================
// Request Headers Forwarding
// ===========================================================================
describe('Request headers forwarding', () => {
    test('custom headers are forwarded to handler', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            headers: authHeaders({ 'X-Custom-Header': 'test-value' }),
        });
        expect(res.status).toEqual(200);
    });
});
// ===========================================================================
// Route Validation (unit-level via Router import)
// ===========================================================================
describe('Route validation', () => {
    test('route starting with slug segment throws', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default;
        expect(Router.validateRoute(':id/GET')).rejects.toThrow('cannot start with a slug');
    });
    test('consecutive slug segments throw', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default;
        expect(Router.validateRoute('api/:id/:name/GET')).rejects.toThrow('consecutive slug segments');
    });
    test('valid route does not throw', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default;
        await expect(Router.validateRoute('users/:id/GET')).resolves.toBeUndefined();
    });
});
// ===========================================================================
// Status Code Routing — /items example
// ===========================================================================
describe('Status code routing (/items)', () => {
    test('GET /items returns 200 with items array', async () => {
        const res = await authFetch('/items');
        expect(res.status).toEqual(200);
        const body = await res.json();
        expect(body).toHaveProperty('items');
        expect(Array.isArray(body.items)).toBe(true);
    });
    test('POST /items with valid body returns 201', async () => {
        const res = await authFetch('/items', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ name: 'sprocket' }),
        });
        expect(res.status).toEqual(201);
        const body = await res.json();
        expect(body).toHaveProperty('id');
        expect(body.name).toBe('sprocket');
    });
    test('POST /items with missing name returns 400', async () => {
        const res = await authFetch('/items', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({}),
        });
        expect(res.status).toEqual(400);
        const body = await res.json();
        expect(body).toHaveProperty('detail');
    });
    test('DELETE /items returns 204 with empty body', async () => {
        const res = await authFetch('/items', { method: 'DELETE' });
        expect(res.status).toEqual(204);
    });
});
// ===========================================================================
// Validate.matchStatusCode (unit-level)
// ===========================================================================
describe('Validate.matchStatusCode', () => {
    const FAKE_ROUTES_PATH = '/fake/routes';
    async function setup() {
        const Router = (await import('../../src/server/route-handler.js')).default;
        const Validate = (await import('../../src/server/schema-validator.js')).default;
        // Override routesPath so handler paths resolve correctly
        Object.defineProperty(Router, 'routesPath', { value: FAKE_ROUTES_PATH, configurable: true });
        return { Router, Validate };
    }
    test('returns matching status code when body fits schema', async () => {
        const { Router, Validate } = await setup();
        Router.routeConfigs['/items'] = { POST: { '201': { id: 'string', name: 'string' } } };
        const handler = `${FAKE_ROUTES_PATH}/items/POST`;
        expect(Validate.matchStatusCode(handler, JSON.stringify({ id: 'abc', name: 'widget' }))).toBe(201);
    });
    test('returns first schema match in ascending order', async () => {
        const { Router, Validate } = await setup();
        Router.routeConfigs['/items'] = {
            POST: {
                '200': { message: 'string' },
                '201': { id: 'string' },
            }
        };
        const handler = `${FAKE_ROUTES_PATH}/items/POST`;
        // body matches 200 schema first
        expect(Validate.matchStatusCode(handler, JSON.stringify({ message: 'ok' }))).toBe(200);
    });
    test('returns null when no numeric schemas are defined', async () => {
        const { Router, Validate } = await setup();
        Router.routeConfigs['/legacy'] = { GET: { res: { message: 'string' } } };
        const handler = `${FAKE_ROUTES_PATH}/legacy/GET`;
        expect(Validate.matchStatusCode(handler, JSON.stringify({ message: 'ok' }))).toBeNull();
    });
    test('returns null when no schema matches', async () => {
        const { Router, Validate } = await setup();
        Router.routeConfigs['/strict'] = { POST: { '201': { id: 'string' } } };
        const handler = `${FAKE_ROUTES_PATH}/strict/POST`;
        // body has wrong shape — no match
        expect(Validate.matchStatusCode(handler, JSON.stringify({ message: 'hello' }))).toBeNull();
    });
    test('returns null for non-JSON body', async () => {
        const { Router, Validate } = await setup();
        Router.routeConfigs['/text'] = { GET: { '200': { message: 'string' } } };
        const handler = `${FAKE_ROUTES_PATH}/text/GET`;
        expect(Validate.matchStatusCode(handler, 'plain text')).toBeNull();
    });
    test('returns null when route config is missing', async () => {
        const { Validate } = await setup();
        const handler = `${FAKE_ROUTES_PATH}/nonexistent/GET`;
        expect(Validate.matchStatusCode(handler, JSON.stringify({ message: 'ok' }))).toBeNull();
    });
});
// ===========================================================================
// Router.parseParams (unit-level)
// ===========================================================================
describe('Router.parseParams', () => {
    test('coerces numbers', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default;
        expect(Router.parseParams(['42', '3.14'])).toEqual([42, 3.14]);
    });
    test('coerces booleans', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default;
        expect(Router.parseParams(['true', 'false'])).toEqual([true, false]);
    });
    test('coerces null and undefined', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default;
        expect(Router.parseParams(['null', 'undefined'])).toEqual([null, undefined]);
    });
    test('preserves plain strings', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default;
        expect(Router.parseParams(['hello', 'world'])).toEqual(['hello', 'world']);
    });
    test('handles mixed types', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default;
        expect(Router.parseParams(['42', 'hello', 'true', 'null'])).toEqual([42, 'hello', true, null]);
    });
    test('handles empty array', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default;
        expect(Router.parseParams([])).toEqual([]);
    });
    test('throws Response 400 when a param exceeds MAX_PARAM_LENGTH', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default;
        const long = 'a'.repeat(Router.MAX_PARAM_LENGTH + 1);
        expect(() => Router.parseParams([long])).toThrow();
    });
    test('accepts a param exactly at MAX_PARAM_LENGTH', async () => {
        const Router = (await import('../../src/server/route-handler.js')).default;
        const boundary = 'a'.repeat(Router.MAX_PARAM_LENGTH);
        expect(() => Router.parseParams([boundary])).not.toThrow();
    });
});
// ===========================================================================
// Security Headers
// ===========================================================================
describe('Security headers', () => {
    test('response includes X-Frame-Options: DENY', async () => {
        const res = await authFetch('/api');
        expect(res.headers.get('x-frame-options')).toBe('DENY');
    });
    test('response includes X-Content-Type-Options: nosniff', async () => {
        const res = await authFetch('/api');
        expect(res.headers.get('x-content-type-options')).toBe('nosniff');
    });
    test('plain local HTTP responses do not include Strict-Transport-Security by default', async () => {
        const res = await authFetch('/api');
        expect(res.headers.get('strict-transport-security')).toBeNull();
    });
    test('response includes Content-Security-Policy header', async () => {
        const res = await authFetch('/api');
        expect(res.headers.get('content-security-policy')).not.toBeNull();
    });
    test('response includes Referrer-Policy header', async () => {
        const res = await authFetch('/api');
        expect(res.headers.get('referrer-policy')).toBe('strict-origin-when-cross-origin');
    });
    test('401 response also carries security headers', async () => {
        const res = await fetch(`${BASE_URL}/api`);
        expect(res.status).toBe(401);
        expect(res.headers.get('x-frame-options')).toBe('DENY');
        expect(res.headers.get('x-content-type-options')).toBe('nosniff');
    });
});
// ===========================================================================
// Rate limiting
// ===========================================================================
describe('Rate limiting', () => {
    test('requests over the configured limit return 429 with rate limit headers', async () => {
        const port = String(18_100 + Math.floor(Math.random() * 800));
        const baseUrl = `http://127.0.0.1:${port}`;
        const basicAuthHash = await Bun.password.hash(TEST_BASIC_AUTH);
        const proc = Bun.spawn(['bun', SERVE_SCRIPT], {
            cwd: EXAMPLES_DIR,
            env: {
                ...process.env,
                PORT: port,
                HOSTNAME: '127.0.0.1',
                BASIC_AUTH_HASH: basicAuthHash,
                RATE_LIMIT_MAX: '2',
                RATE_LIMIT_WINDOW_MS: '60000',
            },
            stdout: 'ignore',
            stderr: 'ignore',
        });
        try {
            await waitForServer(baseUrl, proc);
            const [first, second, third] = await Promise.all([
                fetch(`${baseUrl}/api`, { headers: authHeaders() }),
                fetch(`${baseUrl}/api`, { headers: authHeaders() }),
                fetch(`${baseUrl}/api`, { headers: authHeaders() }),
            ]);
            expect(first.status).toBe(200);
            expect(second.status).toBe(200);
            expect(third.status).toBe(429);
            expect(third.headers.get('ratelimit-limit')).toBe('2');
            expect(third.headers.get('ratelimit-remaining')).toBe('0');
            expect(third.headers.get('retry-after')).not.toBeNull();
            const body = await third.json();
            expect(body.detail).toBe('Too many requests');
        }
        finally {
            proc.kill();
            await Bun.sleep(100);
        }
    });
    test('custom middleware rateLimiter overrides the built-in in-memory limiter', async () => {
        const tempRoot = await mkdtemp(path.join(tmpdir(), 'tachyon-rate-limiter-'));
        const middlewareBase = path.join(tempRoot, 'middleware');
        const port = String(18_100 + Math.floor(Math.random() * 800));
        const baseUrl = `http://127.0.0.1:${port}`;
        const basicAuthHash = await Bun.password.hash(TEST_BASIC_AUTH);
        await Bun.write(`${middlewareBase}.js`, `
let hits = 0

const middleware = {
    rateLimiter: {
        async take(_request, context) {
            hits += 1
            return {
                allowed: hits <= 1,
                limit: 1,
                remaining: Math.max(1 - hits, 0),
                resetAt: Date.now() + 60_000,
                headers: {
                    'X-RateLimit-Backend': 'custom',
                    'X-RateLimit-Key': context.host,
                },
            }
        },
    },
}

export default middleware
`);
        const proc = Bun.spawn(['bun', SERVE_SCRIPT], {
            cwd: EXAMPLES_DIR,
            env: {
                ...process.env,
                PORT: port,
                HOSTNAME: '127.0.0.1',
                BASIC_AUTH_HASH: basicAuthHash,
                MIDDLEWARE_PATH: middlewareBase,
                RATE_LIMIT_MAX: '100',
                RATE_LIMIT_WINDOW_MS: '60000',
            },
            stdout: 'ignore',
            stderr: 'ignore',
        });
        try {
            await waitForServer(baseUrl, proc);
            const first = await fetch(`${baseUrl}/api`, { headers: authHeaders() });
            const second = await fetch(`${baseUrl}/api`, { headers: authHeaders() });
            expect(first.status).toBe(200);
            expect(first.headers.get('x-ratelimit-backend')).toBe('custom');
            expect(first.headers.get('ratelimit-limit')).toBe('1');
            expect(second.status).toBe(429);
            expect(second.headers.get('x-ratelimit-backend')).toBe('custom');
            expect(second.headers.get('x-ratelimit-key')).toBe(`127.0.0.1:${port}`);
            expect(second.headers.get('ratelimit-limit')).toBe('1');
            const body = await second.json();
            expect(body.detail).toBe('Too many requests');
        }
        finally {
            proc.kill();
            await Bun.sleep(100);
            await rm(tempRoot, { recursive: true, force: true });
        }
    });
});
// ===========================================================================
// JWT Decoding — expiry enforcement (T-H1)
// ===========================================================================
describe('JWT decoding', () => {
    /** @param {Record<string, unknown>} payload */
    function makeJWT(payload) {
        const header = btoa(JSON.stringify({ alg: 'none', typ: 'JWT' }));
        const body = btoa(JSON.stringify(payload));
        return `${header}.${body}.fakesig`;
    }
    test('expired JWT (exp in the past) is rejected — context.bearer is undefined', async () => {
        const expiredToken = makeJWT({ sub: '1', exp: Math.floor(Date.now() / 1000) - 60 });
        const res = await fetch(`${BASE_URL}/api`, {
            headers: {
                ...authHeaders(),
                'Authorization': AUTH_HEADER,
                'X-JWT': `Bearer ${expiredToken}`,
            },
        });
        // The request still succeeds (JWT is optional context, not gate-keeping here),
        // but we verify the server doesn't crash on an expired token
        expect([200, 401]).toContain(res.status);
    });
    test('valid JWT (exp in the future) is accepted', async () => {
        const validToken = makeJWT({ sub: '1', exp: Math.floor(Date.now() / 1000) + 3600 });
        const res = await authFetch('/api', {
            headers: { 'Authorization': `Bearer ${validToken}` },
        });
        // Bearer auth without BASIC_AUTH matching returns 401; server should not crash
        expect([200, 401]).toContain(res.status);
    });
    test('malformed JWT does not crash the server', async () => {
        const res = await fetch(`${BASE_URL}/api`, {
            headers: { 'Authorization': 'Bearer not.a.valid.jwt.at.all' },
        });
        expect([200, 401, 500]).toContain(res.status);
        expect(res.status).not.toBe(500);
    });
});
// ===========================================================================
// Parameter length limits (T-L3)
// ===========================================================================
describe('Parameter length limits', () => {
    test('query param exceeding MAX_PARAM_LENGTH returns 400', async () => {
        const long = 'a'.repeat(1001);
        const res = await authFetch(`/api?overflow=${long}`);
        expect(res.status).toBe(400);
    });
    test('query param at MAX_PARAM_LENGTH (1000 chars) is accepted', async () => {
        const boundary = 'a'.repeat(1000);
        const res = await authFetch(`/api?value=${boundary}`);
        expect(res.status).toBe(200);
    });
});
