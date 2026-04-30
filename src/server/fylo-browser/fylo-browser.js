// @ts-check
import path from 'path';
import Fylo from '@d31ma/fylo';
import Router from '../http/route-handler.js';
import logger from '../observability/logger.js';
import { fyloOptions } from '../fylo-options.js';

const browserLogger = logger.child({ scope: 'fylo-browser' });

/** @typedef {import('../http/route-handler.js').default} RouterClass */

/** @param {string | undefined | null} value */
function isTruthy(value) {
    const normalized = value?.trim().toLowerCase();
    return normalized === '1' || normalized === 'true' || normalized === 'yes' || normalized === 'on';
}

/** @param {string | undefined} value @param {string} fallback */
function normalizePublicPath(value, fallback) {
    const source = value && value.trim().length > 0 ? value.trim() : fallback;
    const withLeadingSlash = source.startsWith('/') ? source : `/${source}`;
    if (withLeadingSlash === '/')
        return fallback;
    return withLeadingSlash.replace(/\/+$/, '');
}

const BROWSER_PATH = normalizePublicPath(process.env.YON_DATA_BROWSER_PATH, '/_fylo');
const READONLY_DEFAULT = true;

/** @returns {string} */
function fyloRoot() {
    return process.env.FYLO_ROOT || path.join(process.cwd(), '.fylo-data');
}

/** @returns {boolean} */
function readOnly() {
    const explicit = process.env.YON_DATA_BROWSER_READONLY;
    if (explicit === undefined || explicit === '')
        return READONLY_DEFAULT;
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
    return process.env.FYLO_SCHEMA_DIR || process.env.YON_SCHEMA_DIR;
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
                await Bun.file(path.join(schemaDir, collection, 'history', `${current}.json`)).json()
            );
        }
    } catch {
        // Older applications may still use a flat schema path.
    }

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
        ? schema.$encrypted.filter(/** @returns {f is string} */ (/** @type {unknown} */ f) => typeof f === 'string' && f.length > 0)
        : [];
    schemaCache.set(key, encrypted);
    return encrypted;
}

/**
 * Walk each collection directory in the FYLO root and emit a warning if we
 * find bare JSON files at the top level (siblings of `.fylo/`). FYLO writes
 * documents at `<collection>/.fylo/docs/<2-char>/<TTID>.json`; anything else
 * is almost certainly a developer placing seed data in the wrong spot.
 * @param {string} root
 */
async function warnOnHandPlacedDocs(root) {
    /** @type {string[]} */
    let collectionDirs = [];
    try {
        collectionDirs = await Array.fromAsync(new Bun.Glob('*/').scan({ cwd: root, onlyFiles: false }));
    } catch {
        return;
    }
    for (const dir of collectionDirs) {
        const collection = dir.replace(/\/$/, '');
        if (!collection || collection.startsWith('.')) continue;
        const collectionPath = path.join(root, collection);
        const bareFiles = await Array.fromAsync(new Bun.Glob('*.json').scan({ cwd: collectionPath }));
        if (bareFiles.length > 0) {
            browserLogger.warn(
                `Found ${bareFiles.length} bare *.json file(s) under ${collection}/ — db/collections is FYLO-managed. Move seed data to db/seed/${collection}/ and run "bun run seed".`,
                { collection, files: bareFiles.slice(0, 5) },
            );
        }
    }
}

/**
 * @param {unknown} doc
 * @param {string[]} fields
 * @returns {unknown}
 */
function redactDoc(doc, fields) {
    if (reveal() || !fields.length || !doc || typeof doc !== 'object' || Array.isArray(doc))
        return doc;
    /** @type {Record<string, unknown>} */
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
    // The FYLO shell is self-contained except for Roboto from Google Fonts.
    return "default-src 'self'; "
        + "script-src 'self'; "
        + "style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; "
        + "img-src 'self' data:; "
        + "font-src 'self' https://fonts.gstatic.com; "
        + "connect-src 'self'";
}

/**
 * @param {string} route
 * @param {Response} response
 * @returns {Response}
 */
function withBrowserHeaders(route, response) {
    const headers = new Headers(response.headers);
    headers.set('Cache-Control', 'no-cache, must-revalidate');
    headers.set('Content-Security-Policy', browserContentSecurityPolicy());
    return new Response(response.body, {
        status: response.status,
        statusText: response.statusText,
        headers,
    });
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
    <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Roboto:wght@300;400;500;700&display=swap">
    <link rel="stylesheet" href="${BROWSER_PATH}/app.css">
</head>
<body>
    <main id="fylo-app" class="fylo-shell">
        <header class="fylo-hero">
            <p class="eyebrow md-typescale-label-large">Tachyon &middot; Fylo Browser</p>
            <h1 class="md-typescale-headline-medium">Inspect and query collections stored by <code>@d31ma/fylo</code>.</h1>
            <p class="lede md-typescale-body-large">Read-only by default. Pass <code>YON_DATA_BROWSER_READONLY=false</code> to enable mutation endpoints.</p>
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
                <h2 class="md-typescale-title-large">Query</h2>
                <span id="fylo-query-mode" class="chip">SQL</span>
            </div>
            <p class="muted md-typescale-body-medium">Run a SQL statement against the FYLO root, or a <code>findDocs</code> query against a single collection. Read-only by default.</p>
            <div class="fylo-query">
                <textarea
                    id="fylo-query-source"
                    class="field-control"
                    rows="5"
                    aria-label="SQL statement"
                    placeholder="SQL statement"
                >SELECT * FROM otel-spans</textarea>
                <div class="fylo-query-actions">
                    <button type="button" class="button button-text" id="fylo-query-toggle">Switch to findDocs</button>
                    <button type="button" class="button button-primary" id="fylo-query-run">Run query</button>
                </div>
            </div>
            <div id="fylo-query-results" class="fylo-query-results">
                <p class="muted md-typescale-body-medium">Submit a query to see results here.</p>
            </div>
        </section>
        <section class="fylo-panel">
            <div class="fylo-panel-header">
                <h2 class="md-typescale-title-large">Document detail</h2>
                <span id="fylo-detail-id" class="chip">no document selected</span>
            </div>
            <div id="fylo-detail" class="fylo-detail">
                <p class="muted md-typescale-body-medium">Click a row above to inspect a document and (if WORM) walk its version history.</p>
            </div>
        </section>
    </main>
    <script type="module" src="${BROWSER_PATH}/app.js"></script>
</body>
</html>`;
}

/**
 * @returns {Promise<{ root: string, collections: Array<{ name: string, exists: boolean, docCount?: number, worm?: boolean, error?: string }> }>}
 */
async function listCollections() {
    const root = fyloRoot();
    const fylo = new Fylo(fyloOptions(root));
    /** @type {Array<{ name: string, exists: boolean, docsStored?: number, indexedDocs?: number, worm?: boolean, error?: string }>} */
    const collections = [];
    /** @type {string[]} */
    let names = [];
    try {
        const dir = await Array.fromAsync(new Bun.Glob('*/').scan({ cwd: root, onlyFiles: false }));
        names = dir.map((entry) => entry.replace(/\/$/, '')).filter((name) => name && !name.startsWith('.'));
    } catch {
        names = [];
    }
    for (const name of names) {
        try {
            const info = await fylo.inspectCollection(name);
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

/**
 * @param {string} source
 * @returns {boolean}
 */
function isReadOnlyQuery(source) {
    const trimmed = source.trim().toLowerCase();
    return trimmed.startsWith('select') || trimmed.startsWith('show') || trimmed.startsWith('describe');
}

/**
 * @param {Request} request
 */
async function executeQuery(request) {
    /** @type {{ kind?: string, source?: string, collection?: string, query?: Record<string, unknown> }} */
    let body;
    try {
        body = await request.json();
    } catch {
        return { error: 'invalid JSON body' };
    }
    const fylo = new Fylo(fyloOptions(fyloRoot()));
    if (body.kind === 'sql') {
        const source = (body.source ?? '').toString();
        if (!source.trim())
            return { error: 'source is required for sql queries' };
        if (readOnly() && !isReadOnlyQuery(source))
            return { error: 'browser is read-only; only SELECT/SHOW/DESCRIBE permitted. Set YON_DATA_BROWSER_READONLY=false to enable mutations.' };
        try {
            const result = await fylo.executeSQL(source);
            return { kind: 'sql', source, result };
        } catch (error) {
            return { error: error instanceof Error ? error.message : String(error) };
        }
    }
    if (body.kind === 'find') {
        const collection = (body.collection ?? '').toString();
        if (!collection)
            return { error: 'collection is required for find queries' };
        const query = body.query && typeof body.query === 'object' ? /** @type {Record<string, unknown>} */ (body.query) : {};
        const encryptedFields = await getEncryptedFields(collection);
        /** @type {Array<{ id: string, doc: unknown }>} */
        const docs = [];
        try {
            for await (const entry of fylo.findDocs(collection, /** @type {any} */ (query)).collect()) {
                for (const [id, doc] of Object.entries(/** @type {Record<string, unknown>} */ (entry))) {
                    docs.push({ id, doc: redactDoc(doc, encryptedFields) });
                    if (docs.length >= 200) break;
                }
                if (docs.length >= 200) break;
            }
        } catch (error) {
            return { error: error instanceof Error ? error.message : String(error) };
        }
        return { kind: 'find', collection, docs, encryptedFields, revealed: reveal() };
    }
    return { error: 'kind must be "sql" or "find"' };
}

/**
 * @param {Request} request
 */
async function patchDocument(request) {
    if (readOnly())
        return { error: 'browser is read-only; set YON_DATA_BROWSER_READONLY=false to enable mutations' };
    /** @type {{ collection?: string, id?: string, doc?: Record<string, unknown> }} */
    let body;
    try {
        body = await request.json();
    } catch {
        return { error: 'invalid JSON body' };
    }
    const collection = (body.collection ?? '').toString();
    const id = (body.id ?? '').toString();
    const doc = body.doc;
    if (!collection || !id)
        return { error: 'collection and id are required' };
    if (!doc || typeof doc !== 'object' || Array.isArray(doc))
        return { error: 'doc must be a JSON object' };
    const encryptedFields = await getEncryptedFields(collection);
    if (!reveal() && encryptedFields.length) {
        for (const field of encryptedFields) {
            if (field in doc && /** @type {Record<string, unknown>} */ (doc)[field] === '<encrypted>')
                return { error: `cannot save: encrypted field "${field}" is masked. Set YON_DATA_BROWSER_REVEAL=true to edit` };
        }
    }
    const fylo = new Fylo(fyloOptions(fyloRoot()));
    try {
        const newId = await fylo.patchDoc(collection, { [id]: /** @type {Record<string, any>} */ (doc) });
        return { ok: true, id: newId };
    } catch (error) {
        return { error: error instanceof Error ? error.message : String(error) };
    }
}

/**
 * @param {URL} url
 */
async function deleteDocument(url) {
    if (readOnly())
        return { error: 'browser is read-only; set YON_DATA_BROWSER_READONLY=false to enable mutations' };
    const collection = url.searchParams.get('collection') ?? '';
    const id = url.searchParams.get('id') ?? '';
    if (!collection || !id)
        return { error: 'collection and id query parameters required' };
    const fylo = new Fylo(fyloOptions(fyloRoot()));
    try {
        await fylo.delDoc(collection, id);
        return { ok: true };
    } catch (error) {
        return { error: error instanceof Error ? error.message : String(error) };
    }
}

/**
 * @param {Request} request
 */
async function rebuildCollectionAction(request) {
    if (readOnly())
        return { error: 'browser is read-only; set YON_DATA_BROWSER_READONLY=false to enable rebuilds' };
    /** @type {{ collection?: string }} */
    let body;
    try {
        body = await request.json();
    } catch {
        return { error: 'invalid JSON body' };
    }
    const collection = (body.collection ?? '').toString();
    if (!collection)
        return { error: 'collection is required' };
    const fylo = new Fylo(fyloOptions(fyloRoot()));
    try {
        const result = await fylo.rebuildCollection(collection);
        return { ok: true, result };
    } catch (error) {
        return { error: error instanceof Error ? error.message : String(error) };
    }
}

/**
 * @param {URL} url
 */
async function tailEvents(url) {
    const collection = url.searchParams.get('collection') ?? '';
    if (!collection)
        return { error: 'collection query parameter required' };
    const sinceParam = Number(url.searchParams.get('since') ?? 0);
    const since = Number.isFinite(sinceParam) && sinceParam >= 0 ? Math.trunc(sinceParam) : 0;
    const limitParam = Number(url.searchParams.get('limit') ?? 100);
    const limit = Number.isFinite(limitParam) && limitParam > 0 ? Math.min(Math.trunc(limitParam), 500) : 100;
    const eventsPath = path.join(fyloRoot(), collection, '.fylo', 'events', `${collection}.ndjson`);
    const file = Bun.file(eventsPath);
    if (!await file.exists()) {
        return { collection, events: [], offset: 0, exists: false };
    }
    const size = file.size;
    if (since >= size) {
        return { collection, events: [], offset: size, exists: true };
    }
    let text;
    try {
        text = await file.slice(since).text();
    } catch (error) {
        return { error: error instanceof Error ? error.message : String(error) };
    }
    /** @type {Array<unknown>} */
    const events = [];
    const lines = text.split('\n');
    for (const raw of lines) {
        const line = raw.trim();
        if (!line) continue;
        if (events.length >= limit) break;
        try {
            events.push(JSON.parse(line));
        } catch {
            // ignore malformed line (likely a partial trailing write)
        }
    }
    return { collection, events, offset: size, exists: true };
}

/**
 * @param {URL} url
 */
async function getDocument(url) {
    const collection = url.searchParams.get('collection') ?? '';
    const id = url.searchParams.get('id') ?? '';
    if (!collection || !id)
        return { error: 'collection and id query parameters required' };
    const fylo = new Fylo(fyloOptions(fyloRoot()));
    const encryptedFields = await getEncryptedFields(collection);
    /** @type {Record<string, unknown> | null} */
    let doc = null;
    /** @type {Array<Record<string, unknown>> | null} */
    let history = null;
    /** @type {string | undefined} */
    let docError;
    /** @type {string | undefined} */
    let historyError;
    try {
        const result = await fylo.getDoc(collection, id).once();
        const raw = result && typeof result === 'object' ? /** @type {Record<string, unknown>} */ (result) : null;
        doc = raw ? /** @type {Record<string, unknown>} */ (redactDoc(raw, encryptedFields)) : null;
    } catch (error) {
        docError = error instanceof Error ? error.message : String(error);
    }
    try {
        const raw = await fylo.getHistory(collection, id);
        history = raw.map((entry) => ({
            ...entry,
            data: redactDoc(entry.data, encryptedFields),
        }));
    } catch (error) {
        historyError = error instanceof Error ? error.message : String(error);
    }
    return { collection, id, doc, history, docError, historyError, encryptedFields, revealed: reveal() };
}

/**
 * @param {URL} url
 */
async function listDocuments(url) {
    const collection = url.searchParams.get('collection') ?? '';
    if (!collection)
        return { error: 'collection query parameter required' };
    const limitParam = Number(url.searchParams.get('limit') ?? 25);
    const limit = Number.isFinite(limitParam) && limitParam > 0 ? Math.min(Math.trunc(limitParam), 200) : 25;
    const fylo = new Fylo(fyloOptions(fyloRoot()));
    const encryptedFields = await getEncryptedFields(collection);
    /** @type {Array<{ id: string, doc: unknown }>} */
    const docs = [];
    try {
        for await (const entry of fylo.findDocs(collection, {}).collect()) {
            for (const [id, doc] of Object.entries(/** @type {Record<string, unknown>} */ (entry))) {
                docs.push({ id, doc: redactDoc(doc, encryptedFields) });
                if (docs.length >= limit)
                    break;
            }
            if (docs.length >= limit)
                break;
        }
    } catch (error) {
        return { error: error instanceof Error ? error.message : String(error) };
    }
    return { collection, docs, encryptedFields, revealed: reveal() };
}

/**
 * @param {Request} request
 * @param {unknown} body
 * @returns {Promise<Response>}
 */
async function jsonOk(request, body) {
    return new Response(JSON.stringify(body), {
        status: 200,
        headers: {
            ...Router.getHeaders(request),
            'Content-Type': 'application/json; charset=utf-8',
        },
    });
}

/**
 * @typedef {(handler: (request?: Request, server?: import('bun').Server<any>) => Promise<Response> | Response, options?: { route?: string }) => (request?: Request, server?: import('bun').Server<any>) => Promise<Response>} RouteWrapper
 */

export default class FyloBrowser {
    /** @param {RouteWrapper} [wrapRoute] */
    static registerRoutes(wrapRoute) {
        if (!isTruthy(process.env.YON_DATA_BROWSER_ENABLED)) {
            browserLogger.debug('Data browser disabled (set YON_DATA_BROWSER_ENABLED=true to mount)');
            return;
        }
        browserLogger.info('Fylo browser mounted', {
            path: BROWSER_PATH,
            root: fyloRoot(),
            readOnly: readOnly(),
        });
        // Wishlist: db/collections/ is owned by FYLO. If we spot bare *.json
        // files at the wrong layer (i.e. siblings of .fylo/ instead of under
        // .fylo/docs/<prefix>/), flag them — likely hand-edited seed data
        // that should live under db/seed/ and be imported via `bun run seed`.
        warnOnHandPlacedDocs(fyloRoot()).catch(() => { /* non-fatal */ });

        /** @type {(handler: (request?: Request, server?: import('bun').Server<any>) => Promise<Response> | Response, route: string) => (request?: Request, server?: import('bun').Server<any>) => Promise<Response> | Response} */
        const wrap = wrapRoute
            ? (handler, route) => wrapRoute(handler, { route })
            : (handler, _route) => handler;

        const shellHandler = wrap(async (request = new Request(`http://localhost${BROWSER_PATH}`)) => {
            const response = new Response(shellHtml(), {
                status: 200,
                headers: {
                    ...Router.getHeaders(request),
                    'Content-Type': 'text/html; charset=utf-8',
                },
            });
            return withBrowserHeaders(BROWSER_PATH, response);
        }, BROWSER_PATH);
        Router.reqRoutes[BROWSER_PATH] = { GET: shellHandler };
        Router.reqRoutes[`${BROWSER_PATH}/`] = { GET: shellHandler };

        Router.reqRoutes[`${BROWSER_PATH}/app.css`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/app.css`)) => {
                const { css } = await getAssets();
                return withBrowserHeaders(`${BROWSER_PATH}/app.css`, new Response(css, {
                    status: 200,
                    headers: {
                        ...Router.getHeaders(request),
                        'Content-Type': 'text/css; charset=utf-8',
                    },
                }));
            }, `${BROWSER_PATH}/app.css`),
        };

        Router.reqRoutes[`${BROWSER_PATH}/app.js`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/app.js`)) => {
                const { script } = await getAssets();
                return withBrowserHeaders(`${BROWSER_PATH}/app.js`, new Response(
                    script.replace('__FYLO_BROWSER_PATH__', BROWSER_PATH),
                    {
                        status: 200,
                        headers: {
                            ...Router.getHeaders(request),
                            'Content-Type': 'application/javascript; charset=utf-8',
                        },
                    },
                ));
            }, `${BROWSER_PATH}/app.js`),
        };

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

        Router.reqRoutes[`${BROWSER_PATH}/api/query`] = {
            POST: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/query`, { method: 'POST' })) => {
                return jsonOk(request, await executeQuery(request));
            }, `${BROWSER_PATH}/api/query`),
        };

        Router.reqRoutes[`${BROWSER_PATH}/api/events`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/events`)) => {
                const url = new URL(request.url);
                return jsonOk(request, await tailEvents(url));
            }, `${BROWSER_PATH}/api/events`),
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

        Router.reqRoutes[`${BROWSER_PATH}/api/meta`] = {
            GET: wrap(async (request = new Request(`http://localhost${BROWSER_PATH}/api/meta`)) => {
                return jsonOk(request, {
                    root: fyloRoot(),
                    readOnly: readOnly(),
                    revealed: reveal(),
                    path: BROWSER_PATH,
                });
            }, `${BROWSER_PATH}/api/meta`),
        };

        // Reveal is controlled exclusively by the YON_DATA_BROWSER_REVEAL env var.
        // No per-session reveal endpoint — that route was half-baked because
        // each handler reads `reveal()` from the env at call time, so a
        // successful reveal POST had no effect on subsequent reads. If
        // per-session reveal is needed later, store a signed cookie on the
        // response and have `reveal(request)` read it.
    }
}
