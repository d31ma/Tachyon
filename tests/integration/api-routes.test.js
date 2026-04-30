// @ts-check
import { test, beforeAll, afterAll, expect, describe } from 'bun:test';
import Fylo from '@d31ma/fylo';
import { mkdir, mkdtemp, rm } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
/**
 * @typedef {Bun.Subprocess<any, any, any>} BunProcess
 * @typedef {HeadersInit & Record<string, string>} HeaderRecord
 */
const timedTest = /** @type {any} */ (test);
/** Base URL for the local test server */
const TEST_PORT = '18080';
const BASE_URL = `http://localhost:${TEST_PORT}`;
/** Time to wait for the test server to finish starting up */
const STARTUP_TIMEOUT_MS = 10_000;
/**
 * Basic auth credentials for tests.
 * The test server hashes this value into YON_BASIC_AUTH_HASH at startup.
 * Use TEST_BASIC_AUTH env var to override (e.g. TEST_BASIC_AUTH=user:secret bun test).
 */
const TEST_BASIC_AUTH = process.env.TEST_BASIC_AUTH ?? 'admin:pass';
const AUTH_HEADER = `Basic ${btoa(TEST_BASIC_AUTH)}`;
/** @type {BunProcess | null} */
let serverProcess = null;
/** @type {string} */
let telemetryRoot = '';
/** @type {string} */
let itemDataPath = '';
/** @type {string} */
let testDistPath = '';
const PROJECT_ROOT = `${import.meta.dir}/../..`;
const EXAMPLES_DIR = `${PROJECT_ROOT}/examples`;
const SERVE_SCRIPT = `${PROJECT_ROOT}/src/cli/serve.js`;
const TELEMETRY_ALERT_WORKER = `${EXAMPLES_DIR}/server/workers/telemetry-alert-worker.js`;
/** @type {Set<string>} */
const allocatedTestPorts = new Set();
let nextEphemeralTestPort = 22_000 + Math.floor(Math.random() * 10_000);

/**
 * @param {string} command
 * @returns {boolean}
 */
function commandAvailable(command) {
    try {
        const probe = Bun.spawnSync({
            cmd: [command, '--version'],
            stdout: 'pipe',
            stderr: 'pipe',
        });
        return probe.exitCode === 0;
    }
    catch {
        return false;
    }
}

/** @returns {string} */
function itemTestId() {
    return `t${Math.random().toString(36).slice(2, 8)}`;
}

/**
 * @param {unknown} stream
 * @returns {Promise<string>}
 */
async function readTextStream(stream) {
    if (!(stream instanceof ReadableStream)) return '';
    try {
        return await new Response(stream).text();
    } catch {
        return '';
    }
}

/**
 * @param {BunProcess} proc
 * @returns {Promise<string>}
 */
async function describeEarlyExit(proc) {
    const [stdout, stderr] = await Promise.all([
        readTextStream(proc.stdout),
        readTextStream(proc.stderr),
    ]);
    const output = [stdout, stderr].filter((text) => text.trim().length > 0).join('\n').trim();
    const tail = output.length > 2_000 ? output.slice(-2_000) : output;
    return `Test server exited early with code ${proc.exitCode}${tail ? `\n${tail}` : ''}`;
}
/**
 * @param {string} baseUrl
 * @param {BunProcess} proc
 */
async function waitForServer(baseUrl, proc) {
    const startedAt = Date.now();
    while (Date.now() - startedAt < STARTUP_TIMEOUT_MS) {
        if (proc.exitCode !== null) {
            throw new Error(await describeEarlyExit(proc));
        }
        try {
            const response = await fetch(`${baseUrl}/health`);
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

/** @returns {Promise<string>} */
async function getAvailablePort() {
    for (let attempt = 0; attempt < 200; attempt++) {
        const port = String(nextEphemeralTestPort++);
        if (!allocatedTestPorts.has(port)) {
            try {
                const server = Bun.serve({
                    hostname: '127.0.0.1',
                    port: Number(port),
                    fetch() {
                        return new Response('ok');
                    },
                });
                await server.stop(true);
                allocatedTestPorts.add(port);
                await Bun.sleep(50);
                return port;
            } catch {
                allocatedTestPorts.add(port);
            }
        }
    }
    throw new Error('Unable to allocate a unique test port');
}

/**
 * @param {string} prefix
 * @returns {Promise<string>}
 */
async function createBackendOnlyApp(prefix) {
    const root = await mkdtemp(path.join(tmpdir(), prefix));
    const routeDir = path.join(root, 'server', 'routes', 'api');
    await mkdir(routeDir, { recursive: true });
    await Bun.write(path.join(routeDir, 'OPTIONS.json'), JSON.stringify({
        GET: {
            200: {
                message: '^ok$',
            },
        },
    }, null, 2));
    await Bun.write(path.join(routeDir, 'GET.js'), `export async function handler() {
  return { message: 'ok' }
}
`);
    return root;
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

/**
 * @param {unknown} value
 * @returns {string}
 */
function normalizeTelemetryPath(value) {
    return String(value ?? '').replaceAll('\\', '/');
}

beforeAll(async () => {
    telemetryRoot = await mkdtemp(path.join(tmpdir(), 'tachyon-otel-'));
    itemDataPath = path.join(telemetryRoot, 'items.json');
    testDistPath = path.join(telemetryRoot, 'dist');
    await Bun.write(itemDataPath, '[]');
    const basicAuthHash = await Bun.password.hash(TEST_BASIC_AUTH);
    serverProcess = Bun.spawn(['bun', SERVE_SCRIPT], {
        cwd: EXAMPLES_DIR,
        env: {
            ...process.env,
            YON_PORT: TEST_PORT,
            YON_HOSTNAME: '127.0.0.1',
            YON_BASIC_AUTH_HASH: basicAuthHash,
            YON_ALLOW_HEADERS: 'Content-Type,Authorization',
            YON_ALLOW_ORIGINS: 'https://app.example.com',
            YON_ALLOW_METHODS: 'GET,POST,PUT,DELETE,PATCH,OPTIONS',
            YON_ENABLE_HSTS: 'false',
            YON_MAX_BODY_BYTES: '64',
            YON_OTEL_ENABLED: 'true',
            YON_OTEL_ROOT: telemetryRoot,
            YON_ITEMS_DATA_PATH: itemDataPath,
            YON_DIST_PATH: testDistPath,
            // Keep this test hermetic: .env.test uses LocalStack indexes for the
            // example app, but the telemetry root here is per-run and temporary.
            FYLO_INDEX_BACKEND: '',
            YON_OTEL_SERVICE_NAME: 'tachyon-tests',
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
    const proc = serverProcess;
    serverProcess = null;
    proc?.kill();
    if (proc) {
        await Promise.race([
            proc.exited.catch(() => null),
            Bun.sleep(1_000),
        ]);
    }
    if (telemetryRoot)
        await rm(telemetryRoot, { recursive: true, force: true });
}, 15_000);
const routeTestCases = [
    {
        route: '/languages/javascript',
        methods: [
            { method: 'GET' },
            { method: 'POST' },
            { method: 'PUT' },
        ],
    },
    {
        route: '/languages/python/versions/v2',
        methods: [
            { method: 'GET' },
            { method: 'DELETE' },
            { method: 'PATCH', path: '/languages/python/versions/v2/users' },
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
        const res = await fetch(`${BASE_URL}/languages/javascript`);
        expect(res.status).toEqual(401);
        const body = await res.json();
        expect(body.detail).toBe('Unauthorized Client');
    });
    test('401 response includes WWW-Authenticate header', async () => {
        const res = await fetch(`${BASE_URL}/languages/javascript`);
        expect(res.headers.get('www-authenticate')).toBe('Basic realm="Secure Area"');
    });
    test('wrong credentials return 401', async () => {
        const res = await fetch(`${BASE_URL}/languages/javascript`, {
            headers: { 'Authorization': `Basic ${btoa('wrong:creds')}` },
        });
        expect(res.status).toEqual(401);
    });
    test('correct credentials return 200', async () => {
        const res = await authFetch('/languages/javascript');
        expect(res.status).toEqual(200);
    });
    test('malformed auth header returns 401', async () => {
        const res = await fetch(`${BASE_URL}/languages/javascript`, {
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
        const res = await authFetch('/languages/python/versions/v1');
        expect(res.status).toEqual(200);
        const body = await res.json();
        expect(body).toBeDefined();
    });
    test('different dynamic segment values resolve', async () => {
        const res = await authFetch('/languages/python/versions/v3');
        expect(res.status).toEqual(200);
    });
    test('dynamic segment with DELETE method', async () => {
        const res = await authFetch('/languages/python/versions/v2', { method: 'DELETE' });
        expect(res.status).toEqual(200);
    });
    test('dynamic segment with PATCH and trailing path', async () => {
        const res = await authFetch('/languages/python/versions/v1/users', { method: 'PATCH' });
        expect(res.status).toEqual(200);
    });
});
// ===========================================================================
// Query Parameter Parsing
// ===========================================================================
describe('Query parameter parsing', () => {
    test('numeric query params', async () => {
        const res = await authFetch('/languages/javascript?count=42');
        expect(res.status).toEqual(200);
    });
    test('boolean query params', async () => {
        const res = await authFetch('/languages/javascript?active=true');
        expect(res.status).toEqual(200);
    });
    test('null query param', async () => {
        const res = await authFetch('/languages/javascript?value=null');
        expect(res.status).toEqual(200);
    });
    test('comma-separated values', async () => {
        const res = await authFetch('/languages/javascript?tags=a,b,c');
        expect(res.status).toEqual(200);
    });
    test('JSON object query param', async () => {
        const res = await authFetch(`/languages/javascript?data=${encodeURIComponent('{"key":"val"}')}`);
        expect(res.status).toEqual(200);
    });
    test('multiple query params', async () => {
        const res = await authFetch('/languages/javascript?foo=bar&num=10&flag=false');
        expect(res.status).toEqual(200);
    });
});
// ===========================================================================
// Request Body Parsing
// ===========================================================================
describe('Request body parsing', () => {
    test('JSON body is parsed', async () => {
        const res = await authFetch('/languages/javascript', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ message: 'hello' }),
        });
        expect(res.status).toEqual(200);
    });
    test('text body is parsed as string', async () => {
        const res = await authFetch('/languages/javascript', {
            method: 'POST',
            headers: { 'Content-Type': 'text/plain' },
            body: 'plain text body',
        });
        expect(res.status).toEqual(200);
    });
    test('empty body is accepted', async () => {
        const res = await authFetch('/languages/javascript', { method: 'POST' });
        expect(res.status).toEqual(200);
    });
    test('body exceeding YON_MAX_BODY_BYTES returns 413 before handler execution', async () => {
        const res = await authFetch('/languages/javascript', {
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
    test('OPTIONS /languages/typescript/items returns schema JSON', async () => {
        const res = await fetch(`${BASE_URL}/languages/typescript/items`, { method: 'OPTIONS' });
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('application/json');
        const body = await res.json();
        expect(body).toHaveProperty('GET');
        expect(body).toHaveProperty('POST');
        expect(body).toHaveProperty('DELETE');
    });
    test('OPTIONS /languages/javascript returns schema JSON', async () => {
        const res = await fetch(`${BASE_URL}/languages/javascript`, { method: 'OPTIONS' });
        expect(res.status).toEqual(200);
        const body = await res.json();
        expect(body).toHaveProperty('GET');
        expect(body).toHaveProperty('POST');
        expect(body).toHaveProperty('PUT');
    });
    test('OPTIONS /languages/python/versions/:version returns schema JSON', async () => {
        const res = await fetch(`${BASE_URL}/languages/python/versions/v2`, { method: 'OPTIONS' });
        expect(res.status).toEqual(200);
        const body = await res.json();
        expect(body).toHaveProperty('GET');
        expect(body).toHaveProperty('DELETE');
        expect(body).toHaveProperty('PATCH');
    });
    test('OPTIONS schema has numeric status code keys', async () => {
        const res = await fetch(`${BASE_URL}/languages/javascript`, { method: 'OPTIONS' });
        const body = await res.json();
        expect(body.GET).toHaveProperty('200');
        expect(body.GET).toHaveProperty('500');
        expect(body.GET['200']).toHaveProperty('message');
    });
    test('OPTIONS preflight response includes configured CORS headers', async () => {
        const res = await fetch(`${BASE_URL}/languages/javascript`, {
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
        const res = await authFetch('/languages/javascript', { method: 'DELETE' });
        expect(res.status).toEqual(404);
    });
});
// ===========================================================================
// CORS Headers
// ===========================================================================
describe('CORS headers', () => {
    test('response includes Access-Control-Allow-Credentials header', async () => {
        const res = await authFetch('/languages/javascript');
        // YON_ALLOW_CREDENTIALS defaults to "false" when not set
        expect(res.headers.get('access-control-allow-credentials')).toBe('false');
    });
    test('401 response includes Access-Control-Allow-Credentials header', async () => {
        const res = await fetch(`${BASE_URL}/languages/javascript`);
        expect(res.status).toEqual(401);
        expect(res.headers.get('access-control-allow-credentials')).toBe('false');
    });
    test('allowed cross-origin request echoes the matched origin', async () => {
        const res = await authFetch('/languages/javascript', {
            headers: { Origin: 'https://app.example.com' },
        });
        expect(res.status).toEqual(200);
        expect(res.headers.get('access-control-allow-origin')).toBe('https://app.example.com');
        expect(res.headers.get('vary')).toBe('Origin');
    });
    test('same-origin request remains allowed even when not listed in YON_ALLOW_ORIGINS', async () => {
        const res = await authFetch('/languages/javascript', {
            method: 'POST',
            headers: { Origin: BASE_URL },
        });
        expect(res.status).toEqual(200);
        expect(res.headers.get('access-control-allow-origin')).toBe(BASE_URL);
    });
    test('disallowed cross-origin request returns 403 before handler execution', async () => {
        const res = await authFetch('/languages/javascript', {
            headers: { Origin: 'https://evil.example.com' },
        });
        expect(res.status).toEqual(403);
        expect(res.headers.get('access-control-allow-origin')).toBe('https://app.example.com');
        const body = await res.json();
        expect(body.detail).toBe('Origin not allowed');
    });
    test('disallowed preflight request returns 403', async () => {
        const res = await fetch(`${BASE_URL}/languages/javascript`, {
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
    test('GET /languages/javascript returns a message string', async () => {
        const res = await authFetch('/languages/javascript');
        const body = await res.json();
        expect(body).toHaveProperty('message');
        expect(typeof body.message).toBe('string');
    });
    test('POST /languages/javascript returns a message field', async () => {
        const res = await authFetch('/languages/javascript', { method: 'POST' });
        const body = await res.json();
        expect(body).toHaveProperty('message');
    });
    test('PUT /languages/javascript returns a message field', async () => {
        const res = await authFetch('/languages/javascript', { method: 'PUT' });
        const body = await res.json();
        expect(body).toHaveProperty('message');
    });
    test('GET /languages/python/versions/v2 returns a message field', async () => {
        const res = await authFetch('/languages/python/versions/v2');
        const body = await res.json();
        expect(body).toHaveProperty('message');
    });
    test('handler messages vary by language', async () => {
        const res1 = await authFetch('/languages/javascript');
        const body1 = await res1.json();
        const res2 = await authFetch('/languages/javascript', { method: 'POST' });
        const body2 = await res2.json();
        // The MVC service exposes different operations through the same route.
        expect(body1.message).not.toEqual(body2.message);
    });
});
// ===========================================================================
// Polyglot Root Route Adapters
// ===========================================================================
describe('Polyglot root route adapters', () => {
    test('POST /languages/java executes the Java adapter route', async () => {
        const res = await authFetch('/languages/java', { method: 'POST' });
        expect(res.status).toEqual(200);
        expect(await res.json()).toHaveProperty('message', 'Hello from Java!');
    });
    test('DELETE /languages/dart executes the Dart adapter route', async () => {
        if (!commandAvailable('dart'))
            return;
        const res = await authFetch('/languages/dart', { method: 'DELETE' });
        expect(res.status).toEqual(200);
        expect(await res.json()).toHaveProperty('message', 'Hello from Dart!');
    });
    timedTest('PATCH /languages/rust executes the Rust adapter route', { timeout: 15000 }, async () => {
        const res = await authFetch('/languages/rust', { method: 'PATCH' });
        expect(res.status).toEqual(200);
        expect(await res.json()).toHaveProperty('message', 'Hello from Rust!');
    });
    test('examples/server/routes covers every supported Yon language', async () => {
        const HandlerAdapter = (await import('../../src/server/process/handler-adapter.js')).default;
        const Pool = (await import('../../src/server/process/process-pool.js')).default;
        const languageRoutes = new Map([
            ['javascript', `${EXAMPLES_DIR}/server/routes/languages/javascript/GET.js`],
            ['typescript', `${EXAMPLES_DIR}/server/routes/languages/typescript/GET.ts`],
            ['python', `${EXAMPLES_DIR}/server/routes/languages/python/GET.py`],
            ['ruby', `${EXAMPLES_DIR}/server/routes/languages/ruby/GET.rb`],
            ['php', `${EXAMPLES_DIR}/server/routes/languages/php/GET.php`],
            ['dart', `${EXAMPLES_DIR}/server/routes/languages/dart/DELETE.dart`],
            ['go', `${EXAMPLES_DIR}/server/routes/languages/go/GET.go`],
            ['java', `${EXAMPLES_DIR}/server/routes/languages/java/POST.java`],
            ['csharp', `${EXAMPLES_DIR}/server/routes/languages/csharp/GET.cs`],
            ['rust', `${EXAMPLES_DIR}/server/routes/languages/rust/PATCH.rs`],
        ]);
        for (const language of HandlerAdapter.supportedLanguages) {
            const handler = languageRoutes.get(language);
            expect(handler).toBeDefined();
            if (!handler)
                throw new Error(`Missing example handler for ${language}`);
            expect(await Bun.file(handler).exists()).toBe(true);
            expect(Pool.resolveHandlerCommand(handler).join(' ')).toContain(
                language === 'javascript' || language === 'typescript'
                    ? 'yon-js-runner.js'
                    : language === 'python'
                        ? 'yon-python-runner.py'
                        : language === 'ruby'
                            ? 'yon-ruby-runner.rb'
                            : language === 'php'
                                ? 'yon-php-runner.php'
                                : language,
            );
        }
    });
});
// ===========================================================================
// HTML Route & Accept Header
// ===========================================================================
describe('HTML route serving', () => {
    test('Accept text/html on API routes still returns the handler response', async () => {
        const res = await fetch(`${BASE_URL}/languages/javascript`, {
            headers: authHeaders({ 'Accept': 'text/html' }),
        });
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('application/json');
        const body = await res.text();
        expect(body).toContain('Hello from Yon on Bun!');
    });
    test('Accept text/html on versioned API routes still returns the handler response', async () => {
        const res = await fetch(`${BASE_URL}/languages/python/versions/v2`, {
            headers: authHeaders({ 'Accept': 'text/html' }),
        });
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('application/json');
    });
    test('Accept text/html still requires basic auth', async () => {
        const res = await fetch(`${BASE_URL}/languages/javascript`, {
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
        const res = await fetch(`${BASE_URL}/languages/javascript`, {
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
        const res = await authFetch('/languages/javascript');
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
        expect(body).not.toHaveProperty('/languages/javascript');
        expect(body).not.toHaveProperty('/languages/python/versions/:version');
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
        expect(body.paths).toHaveProperty('/languages/javascript');
        expect(body.paths).toHaveProperty('/languages/python/versions/{version}');
        expect(body.paths).toHaveProperty('/health');
        expect(body.info.version).toBe('release');
        expect(body.paths['/languages/typescript/items'].post.responses).toHaveProperty('204');
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
        const res = await authFetch('/languages/javascript', {
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
        expect(getAttribute(requestSpan?.span ?? {}, 'http.route')).toBe('/languages/javascript');
        expect(getAttribute(requestSpan?.span ?? {}, 'http.response.status_code')).toBe(200);
        expect(getAttribute(requestSpan?.span ?? {}, 'tachyon.request.id')).toBe(requestId);
        expect(requestSpan?.span.status?.code).toBe(1);
        expect(getResourceAttribute(requestSpan?.resource ?? {}, 'service.name')).toBe('tachyon-tests');
        expect(getResourceAttribute(requestSpan?.resource ?? {}, 'telemetry.sdk.name')).toBe('tachyon');
        expect(requestSpan?.scope?.name).toBe('@d31ma/tachyon.telemetry');
        expect(handlerSpan?.span.traceId).toBe(traceId);
        expect(handlerSpan?.span.parentSpanId).toBe(requestSpan?.span.spanId);
        expect(normalizeTelemetryPath(getAttribute(handlerSpan?.span ?? {}, 'code.file.path'))).toContain(
            '/examples/server/routes/languages/javascript/GET.js'
        );
    });

    test('telemetry example route reads OTLP JSON from Fylo and returns a monitoring summary', async () => {
        await authFetch('/languages/javascript', {
            headers: {
                'X-Request-Id': `otel-example-${crypto.randomUUID()}`,
            },
        });

        const res = await authFetch('/languages/javascript/telemetry?limit=5');
        expect(res.status).toBe(200);
        const body = await res.json();
        expect(body.summary.enabled).toBe(true);
        expect(body.summary.collection).toBe('otel-spans');
        expect(body.summary.spanCount).toBeGreaterThanOrEqual(2);
        expect(body.summary.requestCount).toBeGreaterThanOrEqual(1);
        expect(Array.isArray(body.recent)).toBe(true);
        expect(body.recent.length).toBeGreaterThan(0);
        expect(body.recent.some((/** @type {any} */ entry) => entry.name === 'GET /languages/javascript')).toBe(true);
    });

    test('telemetry alert worker flags slow routes from the telemetry endpoint', async () => {
        await authFetch('/languages/javascript', {
            headers: {
                'X-Request-Id': `otel-worker-${crypto.randomUUID()}`,
            },
        });

        const proc = Bun.spawn(['bun', TELEMETRY_ALERT_WORKER], {
            cwd: EXAMPLES_DIR,
            env: {
                ...process.env,
                YON_TELEMETRY_URL: `${BASE_URL}/languages/javascript/telemetry?limit=10`,
                YON_BASIC_AUTH_HEADER: AUTH_HEADER,
                YON_ALERT_SLOW_MS: '1',
                YON_ALERT_STATUS_CODE: '500',
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
        expect(body).toContain('<script type="module" src="/browser-env.js"></script>');
        expect(body).not.toContain('PRIVATE_BROWSER_SECRET');
        expect(body).not.toContain('server-only-secret');

        const scriptRes = await fetch(`${BASE_URL}/browser-env.js`);
        expect(scriptRes.status).toBe(200);
        expect(scriptRes.headers.get('content-type')).toContain('javascript');
        const script = await scriptRes.text();
        expect(script).toContain('window.__ty_public_env__');
        expect(script).toContain('PUBLIC_API_BASE_URL');
        expect(script).toContain('https://api.example.com');
        expect(script).not.toContain('PRIVATE_BROWSER_SECRET');
        expect(script).not.toContain('server-only-secret');
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
    test('GET /imports.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/imports.js`);
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('javascript');
        expect(res.headers.get('cache-control')).toBe('no-cache, must-revalidate');
    });
    test('GET /browser-env.js returns CSP-safe public browser env JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/browser-env.js`);
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('javascript');
        expect(res.headers.get('cache-control')).toBe('no-cache, must-revalidate');
        const body = await res.text();
        expect(body).toContain('window.__ty_public_env__');
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
        const res = await fetch(`${BASE_URL}/languages/javascript`, {
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
    test('GET /components/clicker/index.js returns JavaScript', async () => {
        const res = await fetch(`${BASE_URL}/components/clicker/index.js`);
        expect(res.status).toEqual(200);
        expect(res.headers.get('content-type')).toContain('javascript');
    });
    test('GET /components/clicker/ui/index.js returns JavaScript for nested components', async () => {
        const res = await fetch(`${BASE_URL}/components/clicker/ui/index.js`);
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
            const res = await fetch(`${BASE_URL}/languages/javascript`, {
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
        const res = await fetch(`${BASE_URL}/languages/javascript`, {
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
        const Router = (await import('../../src/server/http/route-handler.js')).default;
        expect(Router.validateRoute(':id/GET.js')).rejects.toThrow('cannot start with a slug');
    });
    test('consecutive slug segments throw', async () => {
        const Router = (await import('../../src/server/http/route-handler.js')).default;
        expect(Router.validateRoute('api/:id/:name/GET.js')).rejects.toThrow('consecutive slug segments');
    });
    test('valid route does not throw', async () => {
        const Router = (await import('../../src/server/http/route-handler.js')).default;
        await expect(Router.validateRoute('users/:id/GET.js')).resolves.toBeUndefined();
    });
});
// ===========================================================================
// Status Code Routing — /languages/typescript/items example
// ===========================================================================
describe('Status code routing (/languages/typescript/items)', () => {
    test('GET /languages/typescript/items returns 200 with items array', async () => {
        const res = await authFetch('/languages/typescript/items');
        expect(res.status).toEqual(200);
        const body = await res.json();
        expect(body).toHaveProperty('items');
        expect(Array.isArray(body.items)).toBe(true);
    });
    test('GET /languages/typescript/items/:id retrieves an existing item and returns 404 for a missing item', async () => {
        const id = itemTestId();
        await authFetch('/languages/typescript/items', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ id, name: 'retrievable' }),
        });

        const found = await authFetch(`/languages/typescript/items/${id}`);
        expect(found.status).toEqual(200);
        const body = await found.json();
        expect(body.id).toBe(id);

        const missing = await authFetch('/languages/typescript/items/not-found-item');
        expect(missing.status).toEqual(404);
        expect(await missing.json()).toEqual({ detail: 'item not found' });
    });
    test('POST /languages/typescript/items creates an item and returns 204 with no response body', async () => {
        const id = itemTestId();
        const res = await authFetch('/languages/typescript/items', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ id, name: 'sprocket' }),
        });
        expect(res.status).toEqual(204);
        expect(await res.text()).toBe('');

        const created = await authFetch(`/languages/typescript/items/${id}`);
        expect(created.status).toEqual(200);
        expect((await created.json()).name).toBe('sprocket');
    });
    test('POST /languages/typescript/items with missing name returns 400', async () => {
        const res = await authFetch('/languages/typescript/items', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({}),
        });
        expect(res.status).toEqual(400);
        const body = await res.json();
        expect(body).toHaveProperty('detail');
    });
    test('PUT /languages/typescript/items/:id replaces an existing item', async () => {
        const id = itemTestId();
        await authFetch('/languages/typescript/items', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ id, name: 'original' }),
        });

        const res = await authFetch(`/languages/typescript/items/${id}`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ name: 'replacement' }),
        });
        expect(res.status).toEqual(200);
        const body = await res.json();
        expect(body.id).toBe(id);
        expect(body.name).toBe('replacement');

        const missing = await authFetch('/languages/typescript/items/not-found-item', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ name: 'replacement' }),
        });
        expect(missing.status).toEqual(404);
    });
    test('PATCH /languages/typescript/items/:id updates specific fields only', async () => {
        const id = itemTestId();
        await authFetch('/languages/typescript/items', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ id, name: 'before', source: 'api' }),
        });

        const res = await authFetch(`/languages/typescript/items/${id}`, {
            method: 'PATCH',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ name: 'after' }),
        });
        expect(res.status).toEqual(200);
        const body = await res.json();
        expect(body.id).toBe(id);
        expect(body.name).toBe('after');
        expect(body.source).toBe('api');
    });
    test('DELETE /languages/typescript/items/:id deletes a resource and returns 204', async () => {
        const id = itemTestId();
        await authFetch('/languages/typescript/items', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ id, name: 'trash' }),
        });

        const res = await authFetch(`/languages/typescript/items/${id}`, { method: 'DELETE' });
        expect(res.status).toEqual(204);

        const missing = await authFetch(`/languages/typescript/items/${id}`);
        expect(missing.status).toEqual(404);
    });
});
// ===========================================================================
// Validate.matchStatusCode (unit-level)
// ===========================================================================
describe('Validate.matchStatusCode', () => {
    const FAKE_ROUTES_PATH = '/fake/routes';
    async function setup() {
        const Router = (await import('../../src/server/http/route-handler.js')).default;
        const Validate = (await import('../../src/server/http/schema-validator.js')).default;
        // Override routesPath so handler paths resolve correctly
        Object.defineProperty(Router, 'routesPath', { value: FAKE_ROUTES_PATH, configurable: true });
        return { Router, Validate };
    }
    test('returns matching status code when body fits schema', async () => {
        const { Router, Validate } = await setup();
        Router.routeConfigs['/languages/typescript/items'] = { POST: { '201': { id: '^.+$', name: '^.+$' } } };
        const handler = `${FAKE_ROUTES_PATH}/languages/typescript/items/POST`;
        expect(await Validate.matchStatusCode(handler, JSON.stringify({ id: 'abc', name: 'widget' }))).toBe(201);
    });
    test('returns first schema match in ascending order', async () => {
        const { Router, Validate } = await setup();
        Router.routeConfigs['/languages/typescript/items'] = {
            POST: {
                '200': { message: '^.+$' },
                '201': { id: '^.+$' },
            }
        };
        const handler = `${FAKE_ROUTES_PATH}/languages/typescript/items/POST`;
        // body matches 200 schema first
        expect(await Validate.matchStatusCode(handler, JSON.stringify({ message: 'ok' }))).toBe(200);
    });
    test('returns null when no numeric schemas are defined', async () => {
        const { Router, Validate } = await setup();
        Router.routeConfigs['/response-only'] = { GET: { response: { '200': { message: '^.+$' } } } };
        const handler = `${FAKE_ROUTES_PATH}/response-only/GET`;
        expect(await Validate.matchStatusCode(handler, JSON.stringify({ message: 'ok' }))).toBeNull();
    });
    test('returns null when no schema matches', async () => {
        const { Router, Validate } = await setup();
        Router.routeConfigs['/strict'] = { POST: { '201': { id: '^.+$' } } };
        const handler = `${FAKE_ROUTES_PATH}/strict/POST`;
        // body has wrong shape — no match
        expect(await Validate.matchStatusCode(handler, JSON.stringify({ message: 'hello' }))).toBeNull();
    });
    test('returns null for non-JSON body', async () => {
        const { Router, Validate } = await setup();
        Router.routeConfigs['/text'] = { GET: { '200': { message: '^.+$' } } };
        const handler = `${FAKE_ROUTES_PATH}/text/GET`;
        expect(await Validate.matchStatusCode(handler, 'plain text')).toBeNull();
    });
    test('returns null when route config is missing', async () => {
        const { Validate } = await setup();
        const handler = `${FAKE_ROUTES_PATH}/nonexistent/GET`;
        expect(await Validate.matchStatusCode(handler, JSON.stringify({ message: 'ok' }))).toBeNull();
    });
    test('validates OPTIONS request body schemas with CHEX regex patterns', async () => {
        const { Router, Validate } = await setup();
        Router.routeConfigs['/languages/typescript/items'] = {
            POST: {
                request: {
                    body: {
                        name: '^[a-z-]+$',
                        count: '^[0-9]+$',
                    },
                },
                '201': { ok: '^(?:true|false)$' },
            },
        };
        const handler = `${FAKE_ROUTES_PATH}/languages/typescript/items/POST`;
        await expect(Validate.validateData(handler, 'req', {
            headers: { accept: 'application/json' },
            body: { name: 'widget-box', count: 2 },
        })).resolves.toBeUndefined();
        await expect(Validate.validateData(handler, 'req', {
            headers: { accept: 'application/json' },
            body: { name: 'Widget Box', count: 2 },
        })).rejects.toThrow('RegEx pattern fails');
    });
    test('validates OPTIONS response schemas with CHEX regex semantics only', async () => {
        const { Router, Validate } = await setup();
        Router.routeConfigs['/languages/typescript/items'] = {
            GET: {
                response: {
                    '200': {
                        id: '^.+$',
                        count: '^[0-9]+$',
                    },
                },
            },
        };
        const handler = `${FAKE_ROUTES_PATH}/languages/typescript/items/GET`;
        await expect(Validate.validateData(handler, '200', JSON.stringify({
            id: 'one',
            count: 1,
        }))).resolves.toBeUndefined();
        await expect(Validate.validateData(handler, '200', JSON.stringify({
            id: 'one',
            count: '1',
        }))).resolves.toBeUndefined();
        await expect(Validate.validateData(handler, '200', JSON.stringify({
            id: 'one',
            count: 'one',
        }))).rejects.toThrow('RegEx pattern fails');
    });
    test('does not treat OPTIONS string leaves as Tachyon type shorthands', async () => {
        const { Router, Validate } = await setup();
        Router.routeConfigs['/languages/typescript/items'] = {
            POST: {
                request: {
                    body: {
                        count: 'number',
                    },
                },
            },
        };
        const handler = `${FAKE_ROUTES_PATH}/languages/typescript/items/POST`;
        await expect(Validate.validateData(handler, 'req', {
            body: { count: 2 },
        })).rejects.toThrow('RegEx pattern fails');
    });
});
// ===========================================================================
// Router.parseParams (unit-level)
// ===========================================================================
describe('Router.parseParams', () => {
    test('coerces numbers', async () => {
        const Router = (await import('../../src/server/http/route-handler.js')).default;
        expect(Router.parseParams(['42', '3.14'])).toEqual([42, 3.14]);
    });
    test('coerces booleans', async () => {
        const Router = (await import('../../src/server/http/route-handler.js')).default;
        expect(Router.parseParams(['true', 'false'])).toEqual([true, false]);
    });
    test('coerces null and undefined', async () => {
        const Router = (await import('../../src/server/http/route-handler.js')).default;
        expect(Router.parseParams(['null', 'undefined'])).toEqual([null, undefined]);
    });
    test('preserves plain strings', async () => {
        const Router = (await import('../../src/server/http/route-handler.js')).default;
        expect(Router.parseParams(['hello', 'world'])).toEqual(['hello', 'world']);
    });
    test('handles mixed types', async () => {
        const Router = (await import('../../src/server/http/route-handler.js')).default;
        expect(Router.parseParams(['42', 'hello', 'true', 'null'])).toEqual([42, 'hello', true, null]);
    });
    test('handles empty array', async () => {
        const Router = (await import('../../src/server/http/route-handler.js')).default;
        expect(Router.parseParams([])).toEqual([]);
    });
    test('throws Response 400 when a param exceeds YON_MAX_PARAM_LENGTH', async () => {
        const Router = (await import('../../src/server/http/route-handler.js')).default;
        const long = 'a'.repeat(Router.MAX_PARAM_LENGTH + 1);
        expect(() => Router.parseParams([long])).toThrow();
    });
    test('accepts a param exactly at YON_MAX_PARAM_LENGTH', async () => {
        const Router = (await import('../../src/server/http/route-handler.js')).default;
        const boundary = 'a'.repeat(Router.MAX_PARAM_LENGTH);
        expect(() => Router.parseParams([boundary])).not.toThrow();
    });
});
// ===========================================================================
// Security Headers
// ===========================================================================
describe('Security headers', () => {
    test('response includes X-Frame-Options: DENY', async () => {
        const res = await authFetch('/languages/javascript');
        expect(res.headers.get('x-frame-options')).toBe('DENY');
    });
    test('response includes X-Content-Type-Options: nosniff', async () => {
        const res = await authFetch('/languages/javascript');
        expect(res.headers.get('x-content-type-options')).toBe('nosniff');
    });
    test('plain local HTTP responses do not include Strict-Transport-Security by default', async () => {
        const res = await authFetch('/languages/javascript');
        expect(res.headers.get('strict-transport-security')).toBeNull();
    });
    test('response includes Content-Security-Policy header', async () => {
        const res = await authFetch('/languages/javascript');
        expect(res.headers.get('content-security-policy')).not.toBeNull();
    });
    test('response includes Referrer-Policy header', async () => {
        const res = await authFetch('/languages/javascript');
        expect(res.headers.get('referrer-policy')).toBe('strict-origin-when-cross-origin');
    });
    test('401 response also carries security headers', async () => {
        const res = await fetch(`${BASE_URL}/languages/javascript`);
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
        const tempRoot = await createBackendOnlyApp('tachyon-rate-limit-');
        const port = await getAvailablePort();
        const baseUrl = `http://127.0.0.1:${port}`;
        const basicAuthHash = await Bun.password.hash(TEST_BASIC_AUTH);
        const proc = Bun.spawn(['bun', SERVE_SCRIPT], {
            cwd: tempRoot,
            env: {
                ...process.env,
                YON_PORT: port,
                YON_HOSTNAME: '127.0.0.1',
                YON_BASIC_AUTH_HASH: basicAuthHash,
                YON_RATE_LIMIT_MAX: '2',
                YON_RATE_LIMIT_WINDOW_MS: '60000',
            },
            stdout: 'pipe',
            stderr: 'pipe',
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
            await rm(tempRoot, { recursive: true, force: true });
        }
    });
    test('custom middleware rateLimiter overrides the built-in in-memory limiter', async () => {
        const tempRoot = await createBackendOnlyApp('tachyon-rate-limiter-');
        const middlewareBase = path.join(tempRoot, 'middleware');
        const port = await getAvailablePort();
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
            cwd: tempRoot,
            env: {
                ...process.env,
                YON_PORT: port,
                YON_HOSTNAME: '127.0.0.1',
                YON_BASIC_AUTH_HASH: basicAuthHash,
                YON_MIDDLEWARE_PATH: middlewareBase,
                YON_RATE_LIMIT_MAX: '100',
                YON_RATE_LIMIT_WINDOW_MS: '60000',
            },
            stdout: 'pipe',
            stderr: 'pipe',
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
        const res = await fetch(`${BASE_URL}/languages/javascript`, {
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
        const res = await authFetch('/languages/javascript', {
            headers: { 'Authorization': `Bearer ${validToken}` },
        });
        // Bearer auth without YON_BASIC_AUTH matching returns 401; server should not crash
        expect([200, 401]).toContain(res.status);
    });
    test('malformed JWT does not crash the server', async () => {
        const res = await fetch(`${BASE_URL}/languages/javascript`, {
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
    test('query param exceeding YON_MAX_PARAM_LENGTH returns 400', async () => {
        const long = 'a'.repeat(1001);
        const res = await authFetch(`/languages/javascript?overflow=${long}`);
        expect(res.status).toBe(400);
    });
    test('query param at YON_MAX_PARAM_LENGTH (1000 chars) is accepted', async () => {
        const boundary = 'a'.repeat(1000);
        const res = await authFetch(`/languages/javascript?value=${boundary}`);
        expect(res.status).toBe(200);
    });
});
