// @ts-check
import path from 'path';
import net from 'node:net';
import { spawn } from 'node:child_process';
import { Fylo } from '../../vendor/fylo/fylo-node.mjs';
import Router from '../http/route-handler.js';
import logger from '../observability/logger.js';
import { fyloOptions } from '../fylo-options.js';

const gatewayLogger = logger.child({ scope: 'fylo-gateway' });

/** @param {string | undefined | null} value */
function isTruthy(value) {
    const normalized = value?.trim().toLowerCase();
    return normalized === '1' || normalized === 'true' || normalized === 'yes' || normalized === 'on';
}

/** @param {string | undefined} value @param {string} fallback */
function normalizePublicPath(value, fallback) {
    const source = value && value.trim().length > 0 ? value.trim() : fallback;
    const withLeadingSlash = source.startsWith('/') ? source : `/${source}`;
    if (withLeadingSlash === '/') return fallback;
    return withLeadingSlash.replace(/\/+$/, '');
}

const BROWSER_PATH = normalizePublicPath(process.env.YON_DATA_BROWSER_PATH, '/_fylo');
const READONLY_DEFAULT = true;
const DEFAULT_EVENT_STREAM_POLL_MS = 1000;
const MIN_EVENT_STREAM_POLL_MS = 250;
const MAX_EVENT_STREAM_POLL_MS = 30000;
const EVENT_STREAM_LIMIT = 500;

/** @param {string} collection */
function isSafeCollectionName(collection) {
    return collection.length > 0
        && collection !== '.'
        && collection !== '..'
        && !collection.includes('/')
        && !collection.includes('\\');
}

/** @param {string} value */
function safeDecodeURIComponent(value) {
    try { return decodeURIComponent(value); } catch { return null; }
}

/** @returns {string} */
function fyloRoot() {
    return process.env.FYLO_ROOT || path.join(process.cwd(), '.fylo-data');
}

/** @returns {boolean} */
function readOnly() {
    const explicit = process.env.YON_DATA_BROWSER_READONLY;
    if (explicit === undefined || explicit === '') return READONLY_DEFAULT;
    return isTruthy(explicit);
}

/** @returns {boolean} */
function reveal() {
    return isTruthy(process.env.YON_DATA_BROWSER_REVEAL);
}

/** @type {Map<string, string[]>} */
const schemaCache = new Map();

/** @returns {string | undefined} */
function schemaRoot() {
    return process.env.FYLO_SCHEMA || process.env.FYLO_SCHEMA_DIR || process.env.YON_SCHEMA_DIR;
}

/**
 * @param {string} collection
 * @returns {Promise<Record<string, unknown> | null>}
 */
async function loadCollectionSchema(collection) {
    const schemaDir = schemaRoot();
    if (!schemaDir) return null;
    try {
        const manifest = /** @type {{ current?: unknown }} */ (
            await Bun.file(path.join(schemaDir, collection, 'manifest.json')).json()
        );
        const current = typeof manifest.current === 'string' ? manifest.current : '';
        if (current) {
            return /** @type {Record<string, unknown>} */ (
                await Bun.file(path.join(schemaDir, collection, 'history', `${current}.schema.json`)).json()
            );
        }
    } catch { /* fall through */ }
    try {
        return /** @type {Record<string, unknown>} */ (
            await Bun.file(path.join(schemaDir, `${collection}.json`)).json()
        );
    } catch {
        return null;
    }
}

/**
 * @param {string} collection
 * @returns {Promise<string[]>}
 */
async function getEncryptedFields(collection) {
    const key = `${schemaRoot() ?? ''}:${collection}`;
    if (schemaCache.has(key)) return /** @type {string[]} */ (schemaCache.get(key));
    const schema = await loadCollectionSchema(collection);
    const encrypted = Array.isArray(schema?.$encrypted)
        ? schema.$encrypted.filter(/** @returns {f is string} */ (f) => typeof f === 'string' && f.length > 0)
        : [];
    schemaCache.set(key, encrypted);
    return encrypted;
}

/**
 * @param {unknown} doc
 * @param {string[]} fields
 * @returns {unknown}
 */
function redactDoc(doc, fields) {
    if (reveal() || !fields.length || !doc || typeof doc !== 'object' || Array.isArray(doc)) return doc;
    const out = { ...(/** @type {Record<string, unknown>} */ (doc)) };
    for (const field of fields) {
        if (field in out) out[field] = '<encrypted>';
    }
    return out;
}

/** @type {Promise<{ css: string, script: string }> | null} */
let assetsPromise = null;

/** @returns {Promise<{ css: string, script: string }>} */
async function getAssets() {
    if (!assetsPromise) {
        assetsPromise = (async () => {
            const dir = path.join(import.meta.dir, '../../runtime/fylo-browser');
            const [css, script] = await Promise.all([
                Bun.file(path.join(dir, 'app.css')).text(),
                Bun.file(path.join(dir, 'app.js')).text(),
            ]);
            return { css, script };
        })();
    }
    return assetsPromise;
}

function browserContentSecurityPolicy() {
    return "default-src 'self'; "
        + "script-src 'self'; "
        + "style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; "
        + "img-src 'self' data:; "
        + "font-src 'self' https://fonts.gstatic.com; "
        + "connect-src 'self'";
}

/** @param {string} route @param {Response} response */
function withBrowserHeaders(route, response) {
    const headers = new Headers(response.headers);
    headers.set('Cache-Control', 'no-cache, must-revalidate');
    headers.set('Content-Security-Policy', browserContentSecurityPolicy());
    return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
}

function shellHtml() {
    return `<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Fylo Browser</title>
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@300;400;500;600;700&display=swap">
    <link rel="stylesheet" href="${BROWSER_PATH}/app.css">
</head>
<body>
    <main id="fylo-app" class="fylo-shell">
        <header class="fylo-hero">
            <p class="eyebrow md-typescale-label-large">Tachyon &middot; Fylo Browser</p>
            <h1 class="md-typescale-headline-medium">Inspect and query collections stored by the <code>fylo</code> binary.</h1>
            <p class="lede md-typescale-body-large">Read-only by default. Pass <code>YON_DATA_BROWSER_READONLY=false</code> to enable mutation endpoints. Query with <a href="https://docs.postgrest.org/en/stable/references/api/tables_views.html#horizontal-filtering" rel="noopener">PostgREST syntax</a>: <code>?field=op.value</code></p>
        </header>
        <section class="fylo-panel">
            <div class="fylo-panel-header">
                <h2 class="md-typescale-title-large">Collections</h2>
                <span id="fylo-root" class="chip">root pending</span>
            </div>
            <div id="fylo-collections" class="fylo-collections">
                <p class="muted md-typescale-body-medium">Loading collections&hellip;</p>
            </div>
        </section>
        <section class="fylo-panel">
            <div class="fylo-panel-header">
                <h2 class="md-typescale-title-large">Documents</h2>
                <span id="fylo-collection-name" class="chip">no collection selected</span>
            </div>
            <div id="fylo-documents" class="fylo-documents">
                <p class="muted md-typescale-body-medium">Pick a collection above.</p>
            </div>
        </section>
        <section class="fylo-panel">
            <div class="fylo-panel-header">
                <h2 class="md-typescale-title-large">Events</h2>
                <span id="fylo-events-status" class="chip">idle</span>
            </div>
            <p class="muted md-typescale-body-medium">Tail the per-collection event journal at <code>events/&lt;collection&gt;.ndjson</code>. Polls every 3s while running.</p>
            <div class="fylo-events-actions">
                <button type="button" class="button button-text" id="fylo-events-toggle">Start tail</button>
                <button type="button" class="button button-text" id="fylo-events-clear">Clear</button>
            </div>
            <ol id="fylo-events" class="fylo-events">
                <li class="muted md-typescale-body-medium">Pick a collection above, then start the tail.</li>
            </ol>
        </section>
        <section class="fylo-panel">
            <div class="fylo-panel-header">
                <h2 class="md-typescale-title-large">Browsable API</h2>
                <span id="fylo-query-mode" class="chip">GET</span>
            </div>
            <p class="muted md-typescale-body-medium">Send Django-style collection requests. Use <code>/_fylo/&lt;collection&gt;/</code> for lists and creates, or <code>/_fylo/&lt;collection&gt;/&lt;id&gt;/</code> for retrieve, update, patch, and delete.</p>
            <div class="fylo-query">
                <div class="fylo-rest-bar">
                    <label class="sr-only" for="fylo-request-method">HTTP method</label>
                    <select id="fylo-request-method" class="field-control fylo-method">
                        <option>GET</option>
                        <option>POST</option>
                        <option>PUT</option>
                        <option>PATCH</option>
                        <option>DELETE</option>
                    </select>
                    <label class="sr-only" for="fylo-request-path">Request URL</label>
                    <input id="fylo-request-path" class="field-control fylo-path" value="${BROWSER_PATH}/" aria-label="Request URL">
                </div>
                <textarea id="fylo-query-source" class="field-control" rows="7" aria-label="JSON request body" placeholder='{"title":"Example"}'></textarea>
                <div class="fylo-query-actions">
                    <button type="button" class="button button-text" id="fylo-query-toggle">Use selected route</button>
                    <button type="button" class="button button-primary" id="fylo-query-run">Send request</button>
                </div>
            </div>
            <div id="fylo-query-results" class="fylo-query-results">
                <p class="muted md-typescale-body-medium">Send a request to inspect the response here.</p>
            </div>
        </section>
        <section class="fylo-panel">
            <div class="fylo-panel-header">
                <h2 class="md-typescale-title-large">Document detail</h2>
                <span id="fylo-detail-id" class="chip">no document selected</span>
            </div>
            <div id="fylo-detail" class="fylo-detail">
                <p class="muted md-typescale-body-medium">Click a row above to inspect a document.</p>
            </div>
        </section>
    </main>
    <script type="module" src="${BROWSER_PATH}/app.js"></script>
</body>
</html>`;
}

// ── Shared fylo binary handles (created lazily) ──────────────────────────
//
// Fylo 26.28 is binary-first. The admin handlers below drive one long-lived
// `fylo exec --loop` subprocess (via the Node shim) instead of an in-process
// engine, and the `/_fylo/v1/*` data plane is reverse-proxied to a `fylo serve`
// subprocess — the backend the browser sync client is built to reconcile with.
// Both are keyed on the active root so tests that switch FYLO_ROOT recreate them.

/** @type {{ root: string, db: Fylo } | null} */
let sharedFylo = null;

/** @returns {Fylo & Record<string, any>} */
function gatewayFylo() {
    const root = fyloRoot();
    if (!sharedFylo || sharedFylo.root !== root) {
        if (sharedFylo) { try { sharedFylo.db.close(); } catch { /* already gone */ } }
        sharedFylo = { root, db: new Fylo(root, fyloOptions(root)) };
        registerCleanup();
    }
    return /** @type {Fylo & Record<string, any>} */ (sharedFylo.db);
}

/** @type {{ root: string, base: string, proc: import('node:child_process').ChildProcess } | null} */
let serveState = null;
/** @type {Promise<string> | null} */
let servePromise = null;

/** Ask the OS for a free loopback port. @returns {Promise<number>} */
function freePort() {
    return new Promise((resolve, reject) => {
        const srv = net.createServer();
        srv.once('error', reject);
        srv.listen(0, '127.0.0.1', () => {
            const addr = srv.address();
            const port = addr && typeof addr === 'object' ? addr.port : 0;
            srv.close(() => resolve(port));
        });
    });
}

/** Poll `/v1/health` until the serve process answers. @param {string} base */
async function waitForServeHealth(base) {
    const deadline = Date.now() + 10000;
    while (Date.now() < deadline) {
        try {
            const res = await fetch(`${base}/v1/health`);
            if (res.ok) return;
        } catch { /* not listening yet */ }
        await new Promise((r) => setTimeout(r, 100));
    }
    throw new Error('fylo serve did not become healthy within 10s');
}

/** Spawn (once, per root) a `fylo serve` backend and return its base URL. */
async function getServeBase() {
    const root = fyloRoot();
    if (serveState && serveState.root === root) return serveState.base;
    if (servePromise) return servePromise;
    servePromise = (async () => {
        if (serveState) { try { serveState.proc.kill(); } catch { /* already gone */ } serveState = null; }
        // ponytail: freePort → spawn has a tiny TOCTOU window; fine for a local
        // loopback dev/admin backend, add a retry loop only if it ever races.
        const port = await freePort();
        const base = `http://127.0.0.1:${port}`;
        const binary = fyloOptions(root).binary ?? 'fylo';
        const args = ['serve', '--root', root, '--host', '127.0.0.1', '--port', String(port), '--allow-anonymous'];
        const corsOrigin = process.env.YON_CORS_ORIGIN;
        if (corsOrigin) args.push('--cors-origin', corsOrigin);
        const proc = spawn(binary, args, { stdio: ['ignore', 'ignore', 'inherit'] });
        proc.on('error', (err) => gatewayLogger.error('fylo serve failed to spawn', { err }));
        await waitForServeHealth(base);
        serveState = { root, base, proc };
        registerCleanup();
        return base;
    })();
    try {
        return await servePromise;
    } finally {
        servePromise = null;
    }
}

let cleanupRegistered = false;
/** Best-effort teardown of the spawned subprocesses when the process exits. */
function registerCleanup() {
    if (cleanupRegistered) return;
    cleanupRegistered = true;
    // ponytail: 'exit'-only teardown; children die with the parent on a clean
    // exit. Add SIGINT/SIGTERM kills here if orphaned processes ever show up.
    process.on('exit', () => {
        try { serveState?.proc.kill(); } catch { /* already gone */ }
        try { sharedFylo?.db.close(); } catch { /* already gone */ }
    });
}

/**
 * Reverse-proxy a `/_fylo/v1/*` request to the `fylo serve` backend, which
 * implements the PostgREST-conformant data plane the browser sync client uses
 * (`/v1/health`, `/v1/exec`, `/v1/:collection/events`).
 * @param {Request} request
 * @returns {Promise<Response>}
 */
async function delegateToOfficialHandler(request) {
    const base = await getServeBase();
    const url = new URL(request.url);
    const rewrittenPath = url.pathname.replace(`${BROWSER_PATH}/v1`, '/v1');
    const headers = new Headers(request.headers);
    headers.delete('host'); // fetch sets the correct Host for the loopback target
    /** @type {RequestInit & { duplex?: 'half' }} */
    const init = { method: request.method, headers, redirect: 'manual' };
    if (request.method !== 'GET' && request.method !== 'HEAD') {
        init.body = request.body;
        init.duplex = 'half';
    }
    return fetch(`${base}${rewrittenPath}${url.search}`, init);
}

// ── API handlers ─────────────────────────────────────────────────────────

/** @returns {Promise<{ root: string, collections: Array<{ name: string, exists: boolean, docCount?: number, worm?: boolean, error?: string }> }>} */
async function listCollections() {
    const root = fyloRoot();
    const fylo = gatewayFylo();
    /** @type {Array<{ name: string, exists: boolean, docsStored?: number, indexedDocs?: number, worm?: boolean, error?: string }>} */
    const collections = [];
    /** @type {string[]} */
    let names = [];
    try {
        const dir = await Array.fromAsync(new Bun.Glob('*/').scan({ cwd: path.join(root, '.collections'), onlyFiles: false }));
        names = dir.map((entry) => entry.replace(/\/$/, '')).filter((name) => name && !name.startsWith('.'));
    } catch {
        names = [];
    }
    for (const name of names) {
        try {
            const info = await fylo[name].inspect();
            collections.push({
                name,
                exists: Boolean(info?.exists),
                docsStored: typeof info?.docsStored === 'number' ? info.docsStored : undefined,
                indexedDocs: typeof info?.indexedDocs === 'number' ? info.indexedDocs : undefined,
                worm: Boolean(info?.worm),
            });
        } catch (error) {
            collections.push({ name, exists: false, error: error instanceof Error ? error.message : String(error) });
        }
    }
    return { root, collections };
}

/** @param {Request} request */
async function patchDocument(request) {
    if (readOnly()) return { error: 'browser is read-only; set YON_DATA_BROWSER_READONLY=false to enable mutations' };
    /** @type {{ collection?: string, id?: string, doc?: Record<string, unknown> }} */
    let body;
    try { body = await request.json(); } catch { return { error: 'invalid JSON body' }; }
    const collection = (body.collection ?? '').toString();
    const id = (body.id ?? '').toString();
    const doc = body.doc;
    if (!collection || !id) return { error: 'collection and id are required' };
    if (!doc || typeof doc !== 'object' || Array.isArray(doc)) return { error: 'doc must be a JSON object' };
    const encryptedFields = await getEncryptedFields(collection);
    if (!reveal() && encryptedFields.length) {
        for (const field of encryptedFields) {
            if (field in doc && /** @type {Record<string, unknown>} */ (doc)[field] === '<encrypted>')
                return { error: `cannot save: encrypted field "${field}" is masked. Set YON_DATA_BROWSER_REVEAL=true to edit` };
        }
    }
    const root = fyloRoot();
    const fylo = gatewayFylo();
    try {
        const newId = await fylo[collection].patch(id, /** @type {Record<string, any>} */ (doc));
        return { ok: true, id: newId };
    } catch (error) {
        return { error: error instanceof Error ? error.message : String(error) };
    }
}

/** @param {URL} url */
async function deleteDocument(url) {
    if (readOnly()) return { error: 'browser is read-only; set YON_DATA_BROWSER_READONLY=false to enable mutations' };
    const collection = url.searchParams.get('collection') ?? '';
    const id = url.searchParams.get('id') ?? '';
    if (!collection || !id) return { error: 'collection and id query parameters required' };
    const root = fyloRoot();
    const fylo = gatewayFylo();
    try {
        await fylo[collection].delete(id);
        return { ok: true };
    } catch (error) {
        return { error: error instanceof Error ? error.message : String(error) };
    }
}

/** @param {Request} request */
async function readJsonObject(request) {
    /** @type {unknown} */
    let body;
    try { body = await request.json(); } catch {
        throw Response.json({ error: 'invalid JSON body' }, { status: 400, headers: Router.getHeaders(request) });
    }
    if (!body || typeof body !== 'object' || Array.isArray(body)) {
        throw Response.json({ error: 'request body must be a JSON object' }, { status: 400, headers: Router.getHeaders(request) });
    }
    return /** @type {Record<string, unknown>} */ (body);
}

/** @param {Request} request @param {unknown} body @param {number} [status] */
function jsonResponse(request, body, status = 200) {
    return new Response(JSON.stringify(body), {
        status,
        headers: { ...Router.getHeaders(request), 'Content-Type': 'application/json; charset=utf-8' },
    });
}

/** @param {Request} request */
function readOnlyResponse(request) {
    if (!readOnly()) return null;
    return jsonResponse(request, { error: 'browser is read-only; set YON_DATA_BROWSER_READONLY=false to enable mutations' }, 403);
}

/**
 * @param {string} collection
 * @param {Record<string, unknown>} doc
 */
async function assertEditableDocument(collection, doc) {
    const encryptedFields = await getEncryptedFields(collection);
    if (!reveal() && encryptedFields.length) {
        for (const field of encryptedFields) {
            if (field in doc && doc[field] === '<encrypted>') {
                throw Response.json(
                    { error: `cannot save: encrypted field "${field}" is masked. Set YON_DATA_BROWSER_REVEAL=true to edit` },
                    { status: 400 },
                );
            }
        }
    }
}

/**
 * @param {Request} request
 * @returns {{ collection: string, id: string | null } | null}
 */
function parseRestDocumentPath(request) {
    const pathname = new URL(request.url).pathname;
    if (!pathname.startsWith(`${BROWSER_PATH}/`)) return null;
    const rest = pathname.slice(BROWSER_PATH.length + 1);
    if (!rest || rest.startsWith('api/') || rest.startsWith('v1/') || rest === 'app.css' || rest === 'app.js') return null;
    const segments = rest.split('/').filter(Boolean).map(safeDecodeURIComponent);
    if (segments.length < 1 || segments.length > 2) return null;
    if (segments.some(segment => segment === null)) return null;
    const safeSegments = /** @type {string[]} */ (segments);
    if (safeSegments.some(segment => !isSafeCollectionName(segment))) return null;
    return { collection: safeSegments[0], id: safeSegments[1] ?? null };
}

/** @param {Request} request */
async function listRestDocuments(request) {
    const route = parseRestDocumentPath(request);
    if (!route || route.id) return jsonResponse(request, { error: 'not found' }, 404);
    const url = new URL(request.url);
    url.searchParams.set('collection', route.collection);
    return jsonResponse(request, await listDocuments(url));
}

/** @param {Request} request */
async function getRestDocument(request) {
    const route = parseRestDocumentPath(request);
    if (!route || !route.id) return jsonResponse(request, { error: 'not found' }, 404);
    const url = new URL(request.url);
    url.searchParams.set('collection', route.collection);
    url.searchParams.set('id', route.id);
    const result = await getDocument(url);
    if (!result.doc && !result.docError) return jsonResponse(request, { error: 'document not found' }, 404);
    return jsonResponse(request, result);
}

/** @param {Request} request */
async function createRestDocument(request) {
    const readonly = readOnlyResponse(request);
    if (readonly) return readonly;
    const route = parseRestDocumentPath(request);
    if (!route || route.id) return jsonResponse(request, { error: 'not found' }, 404);
    const doc = await readJsonObject(request);
    await assertEditableDocument(route.collection, doc);
    const root = fyloRoot();
    const fylo = gatewayFylo();
    try {
        await fylo[route.collection].create();
        const id = await fylo[route.collection].put(/** @type {Record<string, any>} */ (doc));
        return jsonResponse(request, { ok: true, id }, 201);
    } catch (error) {
        return jsonResponse(request, { error: error instanceof Error ? error.message : String(error) }, 400);
    }
}

/** @param {Request} request */
async function updateRestDocument(request) {
    const readonly = readOnlyResponse(request);
    if (readonly) return readonly;
    const route = parseRestDocumentPath(request);
    if (!route || !route.id) return jsonResponse(request, { error: 'not found' }, 404);
    const doc = await readJsonObject(request);
    await assertEditableDocument(route.collection, doc);
    const root = fyloRoot();
    const fylo = gatewayFylo();
    try {
        if (request.method === 'PUT') await fylo[route.collection].create();
        const id = request.method === 'PUT'
            ? await fylo[route.collection].put({ [route.id]: /** @type {Record<string, any>} */ (doc) })
            : await fylo[route.collection].patch(route.id, /** @type {Record<string, any>} */ (doc));
        return jsonResponse(request, { ok: true, id });
    } catch (error) {
        return jsonResponse(request, { error: error instanceof Error ? error.message : String(error) }, 400);
    }
}

/** @param {Request} request */
async function deleteRestDocument(request) {
    const readonly = readOnlyResponse(request);
    if (readonly) return readonly;
    const route = parseRestDocumentPath(request);
    if (!route || !route.id) return jsonResponse(request, { error: 'not found' }, 404);
    const root = fyloRoot();
    const fylo = gatewayFylo();
    try {
        await fylo[route.collection].delete(route.id);
        return new Response(null, { status: 204, headers: Router.getHeaders(request) });
    } catch (error) {
        return jsonResponse(request, { error: error instanceof Error ? error.message : String(error) }, 400);
    }
}

/** @param {Request} request */
async function rebuildCollectionAction(request) {
    if (readOnly()) return { error: 'browser is read-only; set YON_DATA_BROWSER_READONLY=false to enable rebuilds' };
    /** @type {{ collection?: string }} */
    let body;
    try { body = await request.json(); } catch { return { error: 'invalid JSON body' }; }
    const collection = (body.collection ?? '').toString();
    if (!collection) return { error: 'collection is required' };
    const root = fyloRoot();
    const fylo = gatewayFylo();
    try {
        const result = await fylo[collection].rebuild();
        return { ok: true, result };
    } catch (error) {
        return { error: error instanceof Error ? error.message : String(error) };
    }
}

/** @param {URL} url */
async function listDeletedDocuments(url) {
    const collection = url.searchParams.get('collection') ?? '';
    if (!collection) return { error: 'collection query parameter required' };
    if (!isSafeCollectionName(collection)) return { error: 'invalid collection query parameter' };
    const limitParam = Number(url.searchParams.get('limit') ?? 25);
    const limit = Number.isFinite(limitParam) && limitParam > 0 ? Math.min(Math.trunc(limitParam), 200) : 25;
    const offsetParam = Number(url.searchParams.get('offset') ?? 0);
    const offset = Number.isFinite(offsetParam) && offsetParam >= 0 ? Math.trunc(offsetParam) : 0;
    const root = fyloRoot();
    const fylo = gatewayFylo();
    const encryptedFields = await getEncryptedFields(collection);
    /** @type {Array<{ id: string, doc: unknown, deletedAt?: string }>} */
    const docs = [];
    let skipped = 0;
    try {
        const result = /** @type {Record<string, unknown>} */ (await fylo.findDeletedDocs(collection, {}) ?? {});
        for (const [id, doc] of Object.entries(result)) {
            if (skipped < offset) { skipped++; continue; }
            const redacted = redactDoc(doc, encryptedFields);
            const deletedAt = typeof (/** @type {any} */ (doc)?._deleted) === 'number'
                ? new Date(/** @type {any} */ (doc)._deleted).toISOString()
                : undefined;
            docs.push({ id, doc: redacted, deletedAt });
            if (docs.length >= limit) break;
        }
    } catch (error) {
        return { error: error instanceof Error ? error.message : String(error) };
    }
    return { collection, docs, encryptedFields, revealed: reveal() };
}

/** @param {Request} request */
async function restoreDocumentAction(request) {
    if (readOnly()) return { error: 'browser is read-only; set YON_DATA_BROWSER_READONLY=false to enable restores' };
    /** @type {{ collection?: string, id?: string }} */
    let body;
    try { body = await request.json(); } catch { return { error: 'invalid JSON body' }; }
    const collection = (body.collection ?? '').toString();
    const id = (body.id ?? '').toString();
    if (!collection || !id) return { error: 'collection and id are required' };
    if (!isSafeCollectionName(collection)) return { error: 'invalid collection name' };
    const root = fyloRoot();
    const fylo = gatewayFylo();
    try {
        const restoredId = await fylo[collection].restore(id);
        return { ok: true, id: restoredId };
    } catch (error) {
        return { error: error instanceof Error ? error.message : String(error) };
    }
}

/** @param {URL} url */
async function tailEvents(url) {
    const collection = url.searchParams.get('collection') ?? '';
    if (!collection) return { error: 'collection query parameter required' };
    if (!isSafeCollectionName(collection)) return { error: 'invalid collection query parameter' };
    const sinceRaw = url.searchParams.get('since') ?? '0';
    const sinceParam = sinceRaw === 'latest' ? Number.MAX_SAFE_INTEGER : Number(sinceRaw);
    const since = Number.isFinite(sinceParam) && sinceParam >= 0 ? Math.trunc(sinceParam) : 0;
    const limitParam = Number(url.searchParams.get('limit') ?? 100);
    const limit = Number.isFinite(limitParam) && limitParam > 0 ? Math.min(Math.trunc(limitParam), 500) : 100;
    return readEventJournal(collection, since, limit);
}

/**
 * @param {string} collection @param {number} since @param {number} limit
 */
async function readEventJournal(collection, since, limit) {
    const eventsPath = path.join(fyloRoot(), '.collections', collection, 'events', `${collection}.ndjson`);
    const file = Bun.file(eventsPath);
    if (!await file.exists()) return { collection, events: [], offset: 0, exists: false };
    const size = file.size;
    if (since >= size) return { collection, events: [], offset: size, exists: true };
    let text;
    try { text = await file.slice(since).text(); } catch (error) {
        return { error: error instanceof Error ? error.message : String(error) };
    }
    /** @type {Array<unknown>} */
    const events = [];
    let consumedBytes = 0;
    const encoder = new TextEncoder();
    const lines = text.split('\n');
    const hasTrailingNewline = text.endsWith('\n');
    if (lines.length > 0 && lines.at(-1) === '') {
        lines.pop();
    } else if (lines.length > 0 && !hasTrailingNewline) {
        lines.pop();
    }
    for (const raw of lines) {
        if (events.length >= limit) break;
        const lineBytes = encoder.encode(`${raw}\n`).byteLength;
        const line = raw.trim();
        if (!line) { consumedBytes += lineBytes; continue; }
        try { events.push(JSON.parse(line)); } catch { /* ignore partial writes */ }
        consumedBytes += lineBytes;
    }
    return { collection, events, offset: since + consumedBytes, exists: true };
}

/** @param {number} value */
function clampEventStreamPollMs(value) {
    if (!Number.isFinite(value) || value <= 0) return DEFAULT_EVENT_STREAM_POLL_MS;
    return Math.max(MIN_EVENT_STREAM_POLL_MS, Math.min(Math.trunc(value), MAX_EVENT_STREAM_POLL_MS));
}

/**
 * @param {string | undefined} id
 * @param {string} event
 * @param {unknown} data
 * @returns {string}
 */
function eventStreamFrame(id, event, data) {
    const idLine = id ? `id: ${id}\n` : '';
    const dataLines = JSON.stringify(data).split(/\r?\n/).map((line) => `data: ${line}`).join('\n');
    return `${idLine}event: ${event}\n${dataLines}\n\n`;
}

/** @param {Request} request */
async function streamEvents(request) {
    const url = new URL(request.url);
    const collection = url.searchParams.get('collection') ?? '';
    if (!collection) return jsonResponse(request, { error: 'collection query parameter required' }, 400);
    if (!isSafeCollectionName(collection)) return jsonResponse(request, { error: 'invalid collection query parameter' }, 400);
    const sinceRaw = url.searchParams.get('since') ?? '0';
    const sinceParam = sinceRaw === 'latest' ? Number.MAX_SAFE_INTEGER : Number(sinceRaw);
    let offset = Number.isFinite(sinceParam) && sinceParam >= 0 ? Math.trunc(sinceParam) : 0;
    const pollParam = Number(url.searchParams.get('poll') ?? DEFAULT_EVENT_STREAM_POLL_MS);
    const pollMs = clampEventStreamPollMs(pollParam);
    const encoder = new TextEncoder();
    let timer = /** @type {ReturnType<typeof setInterval> | null} */ (null);
    let closed = false;
    let inFlight = false;
    const stream = new ReadableStream({
        start(controller) {
            const send = (/** @type {string} */ frame) => { if (!closed) controller.enqueue(encoder.encode(frame)); };
            const close = () => {
                if (closed) return;
                closed = true;
                if (timer) clearInterval(timer);
                try { controller.close(); } catch { /* browser may have closed */ }
            };
            request.signal?.addEventListener('abort', close, { once: true });
            send(`retry: ${Math.max(1000, pollMs)}\n: fylo stream connected\n\n`);
            const tick = async () => {
                if (closed || inFlight) return;
                inFlight = true;
                try {
                    const payload = await readEventJournal(collection, offset, EVENT_STREAM_LIMIT);
                    if ('error' in payload) { send(eventStreamFrame(undefined, 'fylo.error', payload)); return; }
                    offset = typeof payload.offset === 'number' ? payload.offset : offset;
                    if (Array.isArray(payload.events) && payload.events.length > 0) {
                        send(eventStreamFrame(String(offset), 'fylo.events', payload));
                    }
                } catch (error) {
                    send(eventStreamFrame(undefined, 'fylo.error', {
                        collection, error: error instanceof Error ? error.message : String(error),
                    }));
                } finally { inFlight = false; }
            };
            tick();
            timer = setInterval(tick, pollMs);
        },
        cancel() { closed = true; if (timer) clearInterval(timer); },
    });
    return new Response(stream, {
        status: 200,
        headers: {
            ...Router.getHeaders(request),
            'Content-Type': 'text/event-stream; charset=utf-8',
            'Cache-Control': 'no-cache, no-transform',
            'Connection': 'keep-alive',
            'X-Accel-Buffering': 'no',
        },
    });
}

/** @param {URL} url */
async function getDocument(url) {
    const collection = url.searchParams.get('collection') ?? '';
    const id = url.searchParams.get('id') ?? '';
    if (!collection || !id) return { error: 'collection and id query parameters required' };
    const root = fyloRoot();
    const fylo = gatewayFylo();
    const encryptedFields = await getEncryptedFields(collection);
    /** @type {Record<string, unknown> | null} */
    let doc = null;
    /** @type {string | undefined} */
    let docError;
    try {
        const result = await fylo[collection].latest(id);
        const resultObject = result && typeof result === 'object' ? /** @type {Record<string, unknown>} */ (result) : null;
        const raw = resultObject && id in resultObject && resultObject[id] && typeof resultObject[id] === 'object'
            ? /** @type {Record<string, unknown>} */ (resultObject[id])
            : resultObject;
        doc = raw ? /** @type {Record<string, unknown>} */ (redactDoc(raw, encryptedFields)) : null;
    } catch (error) {
        docError = error instanceof Error ? error.message : String(error);
    }
    return { collection, id, doc, docError, encryptedFields, revealed: reveal() };
}

// ── PostgREST-style query parameter parsing ──────────────────────────────

/** @type {Set<string>} */
const POSTGREST_OPS = new Set(['eq', 'neq', 'gt', 'gte', 'lt', 'lte', 'like', 'ilike', 'in', 'is']);

/** @type {Set<string>} */
const RESERVED_PARAMS = new Set(['collection', 'limit', 'offset', 'select', 'order']);

/**
 * @param {string} raw
 * @returns {{ [op: string]: unknown } | null}
 */
function parsePostgrestValue(raw) {
    const dotIndex = raw.indexOf('.');
    if (dotIndex === -1) return null;
    const head = raw.slice(0, dotIndex);
    const tail = raw.slice(dotIndex + 1);
    if (head === 'not') {
        const inner = parsePostgrestValue(tail);
        if (!inner) return null;
        /** @type {Record<string, string>} */
        const inverses = { $eq: '$neq', $neq: '$eq', $lt: '$gte', $lte: '$gt', $gt: '$lte', $gte: '$lt' };
        const [[op, val]] = Object.entries(inner);
        const inverse = inverses[op];
        return inverse ? { [inverse]: val } : { $not: inner };
    }
    if (!POSTGREST_OPS.has(head)) return null;
    if (head === 'in') {
        const match = tail.match(/^\((.+)\)$/);
        if (!match) return null;
        const values = /** @type {string[]} */ ([]);
        let current = '';
        let inQuote = false;
        for (const ch of match[1]) {
            if (ch === '"') { inQuote = !inQuote; continue; }
            if (ch === ',' && !inQuote) { values.push(current.trim()); current = ''; continue; }
            current += ch;
        }
        if (current.trim()) values.push(current.trim());
        return { $in: values };
    }
    if (head === 'is') {
        if (tail === 'null') return { $is: null };
        if (tail === 'true') return { $is: true };
        if (tail === 'false') return { $is: false };
        return null;
    }
    if (head === 'like' || head === 'ilike') {
        return { [`$${head}`]: tail.replace(/\*/g, '%') };
    }
    return { [`$${head}`]: tail };
}

/**
 * @param {string} raw
 * @returns {Array<{ field: string, desc: boolean }>}
 */
function parseOrder(raw) {
    return raw.split(',').map((segment) => {
        const trimmed = segment.trim();
        if (!trimmed) return null;
        const parts = trimmed.split('.');
        const field = parts[0];
        const desc = parts.includes('desc');
        return field ? { field, desc } : null;
    }).filter(/** @returns {entry is { field: string, desc: boolean }} */ (entry) => entry !== null);
}

/**
 * @param {unknown} doc @param {string[] | null} fields
 * @returns {unknown}
 */
function selectFields(doc, fields) {
    if (!fields || !doc || typeof doc !== 'object' || Array.isArray(doc)) return doc;
    /** @type {Record<string, unknown>} */
    const out = {};
    for (const field of fields) {
        if (field in /** @type {Record<string, unknown>} */ (doc)) {
            out[field] = /** @type {Record<string, unknown>} */ (doc)[field];
        }
    }
    return out;
}

/**
 * @param {unknown} doc
 * @param {Array<{ field: string, filter: Record<string, unknown> }>} filters
 * @returns {boolean}
 */
function matchesAllFilters(doc, filters) {
    if (!doc || typeof doc !== 'object') return false;
    const record = /** @type {Record<string, unknown>} */ (doc);
    for (const { field, filter } of filters) {
        const value = record[field];
        for (const [op, expected] of Object.entries(filter)) {
            switch (op) {
                case '$eq': if (String(value) !== String(expected)) return false; break;
                case '$neq': if (String(value) === String(expected)) return false; break;
                case '$gt': if (!(String(value) > String(expected))) return false; break;
                case '$gte': if (!(String(value) >= String(expected))) return false; break;
                case '$lt': if (!(String(value) < String(expected))) return false; break;
                case '$lte': if (!(String(value) <= String(expected))) return false; break;
                case '$in': if (!Array.isArray(expected) || !expected.some((v) => String(value) === String(v))) return false; break;
                case '$is': {
                    if (expected === null && value != null) return false;
                    if (expected === true && value !== true) return false;
                    if (expected === false && value !== false) return false;
                    break;
                }
                case '$like': {
                    const escaped = String(expected).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
                    const pattern = escaped.replace(/%/g, '.*');
                    if (!new RegExp(`^${pattern}$`).test(String(value ?? ''))) return false;
                    break;
                }
                case '$ilike': {
                    const escaped = String(expected).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
                    const pattern = escaped.replace(/%/g, '.*');
                    if (!new RegExp(`^${pattern}$`, 'i').test(String(value ?? ''))) return false;
                    break;
                }
                default: break;
            }
        }
    }
    return true;
}

/** @param {URL} url */
async function listDocuments(url) {
    const collection = url.searchParams.get('collection') ?? '';
    if (!collection) return { error: 'collection query parameter required' };
    const limitParam = Number(url.searchParams.get('limit') ?? 25);
    const limit = Number.isFinite(limitParam) && limitParam > 0 ? Math.min(Math.trunc(limitParam), 200) : 25;
    const offsetParam = Number(url.searchParams.get('offset') ?? 0);
    const offset = Number.isFinite(offsetParam) && offsetParam >= 0 ? Math.trunc(offsetParam) : 0;
    const selectParam = url.searchParams.get('select');
    /** @type {string[] | null} */
    const select = selectParam ? selectParam.split(',').map((s) => s.trim()).filter(Boolean) : null;
    const orderParam = url.searchParams.get('order');
    const order = orderParam ? parseOrder(orderParam) : null;
    /** @type {Array<{ field: string, filter: Record<string, unknown> }>} */
    const filters = [];
    for (const [key, value] of url.searchParams.entries()) {
        if (RESERVED_PARAMS.has(key)) continue;
        const filter = parsePostgrestValue(value);
        if (filter) filters.push({ field: key, filter });
    }
    /** @type {Record<string, unknown>} */
    const query = filters.length
        ? { $ops: [{ [filters[0].field]: filters[0].filter }], $limit: (limit + offset) * (filters.length > 1 ? 4 : 1) }
        : {};
    const root = fyloRoot();
    const fylo = gatewayFylo();
    const encryptedFields = await getEncryptedFields(collection);
    /** @type {Array<{ id: string, doc: unknown }>} */
    const docs = [];
    let skipped = 0;
    try {
        const result = /** @type {Record<string, unknown>} */ (await fylo[collection].find(/** @type {any} */ (query)) ?? {});
        for (const [id, doc] of Object.entries(result)) {
            if (filters.length > 1 && !matchesAllFilters(doc, filters)) continue;
            if (skipped < offset) { skipped++; continue; }
            const redacted = redactDoc(doc, encryptedFields);
            docs.push({ id, doc: selectFields(redacted, select) });
            if (docs.length >= limit) break;
        }
    } catch (error) {
        return { error: error instanceof Error ? error.message : String(error) };
    }
    if (order && order.length) {
        docs.sort((a, b) => {
            for (const { field, desc } of order) {
                const aDoc = /** @type {Record<string, unknown>} */ (a.doc);
                const bDoc = /** @type {Record<string, unknown>} */ (b.doc);
                const aVal = aDoc?.[field];
                const bVal = bDoc?.[field];
                if (aVal === bVal) continue;
                if (aVal == null) return desc ? -1 : 1;
                if (bVal == null) return desc ? 1 : -1;
                const cmp = aVal < bVal ? -1 : 1;
                return desc ? -cmp : cmp;
            }
            return 0;
        });
    }
    return { collection, docs, encryptedFields, revealed: reveal() };
}

/** @param {Request} request @param {unknown} body */
async function jsonOk(request, body) {
    return new Response(JSON.stringify(body), {
        status: 200,
        headers: { ...Router.getHeaders(request), 'Content-Type': 'application/json; charset=utf-8' },
    });
}

/**
 * @typedef {(handler: (request?: Request, server?: import('bun').Server<any>) => Promise<Response> | Response, options?: { route?: string }) => (request?: Request, server?: import('bun').Server<any>) => Promise<Response>} RouteWrapper
 */

export default class FyloGateway {
    /** @param {RouteWrapper} [wrapRoute] */
    static registerRoutes(wrapRoute) {
        if (!isTruthy(process.env.YON_DATA_BROWSER_ENABLED)) {
            gatewayLogger.debug('Data browser disabled (set YON_DATA_BROWSER_ENABLED=true to mount)');
            return;
        }
        gatewayLogger.info('Fylo gateway mounted', { path: BROWSER_PATH, root: fyloRoot(), readOnly: readOnly() });

        /** @type {(handler: (request?: Request, server?: import('bun').Server<any>) => Promise<Response> | Response, route: string) => (request?: Request, server?: import('bun').Server<any>) => Promise<Response> | Response} */
        const wrap = wrapRoute
            ? (handler, route) => wrapRoute(handler, { route })
            : (handler, _route) => handler;

        // ── Browser UI shell ──────────────────────────────────────────
        const shellHandler = wrap(async (request = new Request(`http://localhost${BROWSER_PATH}`)) => {
            return withBrowserHeaders(BROWSER_PATH, new Response(shellHtml(), {
                status: 200,
                headers: { ...Router.getHeaders(request), 'Content-Type': 'text/html; charset=utf-8' },
            }));
        }, BROWSER_PATH);
        Router.reqRoutes[BROWSER_PATH] = { GET: shellHandler };
        Router.reqRoutes[`${BROWSER_PATH}/`] = { GET: shellHandler };

        // ── Static assets ─────────────────────────────────────────────
        Router.reqRoutes[`${BROWSER_PATH}/app.css`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/app.css`)) => {
                const { css } = await getAssets();
                return withBrowserHeaders(`${BROWSER_PATH}/app.css`, new Response(css, {
                    status: 200, headers: { ...Router.getHeaders(request), 'Content-Type': 'text/css; charset=utf-8' },
                }));
            }, `${BROWSER_PATH}/app.css`),
        };
        Router.reqRoutes[`${BROWSER_PATH}/app.js`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/app.js`)) => {
                const { script } = await getAssets();
                return withBrowserHeaders(`${BROWSER_PATH}/app.js`, new Response(
                    script.replace('__FYLO_BROWSER_PATH__', BROWSER_PATH),
                    { status: 200, headers: { ...Router.getHeaders(request), 'Content-Type': 'application/javascript; charset=utf-8' } },
                ));
            }, `${BROWSER_PATH}/app.js`),
        };

        // ── FYLO data plane (/_fylo/v1/*) ─────────────────────────────
        // Reverse-proxied to a spawned `fylo serve` backend (binary-first)
        Router.reqRoutes[`${BROWSER_PATH}/v1`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/v1`)) => {
                return delegateToOfficialHandler(request);
            }, `${BROWSER_PATH}/v1`),
            POST: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/v1`, { method: 'POST' })) => {
                return delegateToOfficialHandler(request);
            }, `${BROWSER_PATH}/v1`),
            PATCH: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/v1`, { method: 'PATCH' })) => {
                return delegateToOfficialHandler(request);
            }, `${BROWSER_PATH}/v1`),
            DELETE: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/v1`, { method: 'DELETE' })) => {
                return delegateToOfficialHandler(request);
            }, `${BROWSER_PATH}/v1`),
        };
        Router.reqRoutes[`${BROWSER_PATH}/v1/*`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/v1/items/`)) => {
                return delegateToOfficialHandler(request);
            }, `${BROWSER_PATH}/v1/*`),
            POST: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/v1/items/`, { method: 'POST' })) => {
                return delegateToOfficialHandler(request);
            }, `${BROWSER_PATH}/v1/*`),
            PATCH: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/v1/items/id`, { method: 'PATCH' })) => {
                return delegateToOfficialHandler(request);
            }, `${BROWSER_PATH}/v1/*`),
            DELETE: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/v1/items/id`, { method: 'DELETE' })) => {
                return delegateToOfficialHandler(request);
            }, `${BROWSER_PATH}/v1/*`),
        };

        // ── Tachyon-specific API routes ───────────────────────────────
        Router.reqRoutes[`${BROWSER_PATH}/api/collections`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/collections`)) => {
                return jsonOk(request, await listCollections());
            }, `${BROWSER_PATH}/api/collections`),
        };
        Router.reqRoutes[`${BROWSER_PATH}/api/docs`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/docs`)) => {
                const url = new URL(request.url);
                return jsonOk(request, await listDocuments(url));
            }, `${BROWSER_PATH}/api/docs`),
        };
        Router.reqRoutes[`${BROWSER_PATH}/api/doc`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/doc`)) => {
                const url = new URL(request.url);
                return jsonOk(request, await getDocument(url));
            }, `${BROWSER_PATH}/api/doc`),
        };
        Router.reqRoutes[`${BROWSER_PATH}/api/events`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/events`)) => {
                const url = new URL(request.url);
                return jsonOk(request, await tailEvents(url));
            }, `${BROWSER_PATH}/api/events`),
        };
        Router.reqRoutes[`${BROWSER_PATH}/api/events/stream`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/events/stream`)) => {
                return streamEvents(request);
            }, `${BROWSER_PATH}/api/events/stream`),
        };
        Router.reqRoutes[`${BROWSER_PATH}/api/patch`] = {
            POST: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/patch`, { method: 'POST' })) => {
                return jsonOk(request, await patchDocument(request));
            }, `${BROWSER_PATH}/api/patch`),
        };
        Router.reqRoutes[`${BROWSER_PATH}/api/delete`] = {
            DELETE: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/delete`, { method: 'DELETE' })) => {
                const url = new URL(request.url);
                return jsonOk(request, await deleteDocument(url));
            }, `${BROWSER_PATH}/api/delete`),
        };
        Router.reqRoutes[`${BROWSER_PATH}/api/rebuild`] = {
            POST: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/rebuild`, { method: 'POST' })) => {
                return jsonOk(request, await rebuildCollectionAction(request));
            }, `${BROWSER_PATH}/api/rebuild`),
        };
        Router.reqRoutes[`${BROWSER_PATH}/api/deleted`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/deleted`)) => {
                const url = new URL(request.url);
                return jsonOk(request, await listDeletedDocuments(url));
            }, `${BROWSER_PATH}/api/deleted`),
        };
        Router.reqRoutes[`${BROWSER_PATH}/api/restore`] = {
            POST: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/restore`, { method: 'POST' })) => {
                return jsonOk(request, await restoreDocumentAction(request));
            }, `${BROWSER_PATH}/api/restore`),
        };
        Router.reqRoutes[`${BROWSER_PATH}/api/meta`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/meta`)) => {
                return jsonOk(request, { root: fyloRoot(), readOnly: readOnly(), revealed: reveal(), path: BROWSER_PATH });
            }, `${BROWSER_PATH}/api/meta`),
        };

        // ── REST-style collection paths ───────────────────────────────
        Router.reqRoutes[`${BROWSER_PATH}/*`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/items/`)) => {
                const route = parseRestDocumentPath(request);
                if (!route) return jsonResponse(request, { error: 'not found' }, 404);
                return route.id ? getRestDocument(request) : listRestDocuments(request);
            }, `${BROWSER_PATH}/*`),
            POST: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/items/`, { method: 'POST' })) => {
                return createRestDocument(request);
            }, `${BROWSER_PATH}/*`),
            PUT: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/items/id`, { method: 'PUT' })) => {
                return updateRestDocument(request);
            }, `${BROWSER_PATH}/*`),
            PATCH: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/items/id`, { method: 'PATCH' })) => {
                return updateRestDocument(request);
            }, `${BROWSER_PATH}/*`),
            DELETE: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/items/id`, { method: 'DELETE' })) => {
                return deleteRestDocument(request);
            }, `${BROWSER_PATH}/*`),
        };
    }
}
