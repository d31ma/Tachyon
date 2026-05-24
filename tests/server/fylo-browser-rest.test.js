// @ts-check
import { afterEach, beforeEach, expect, test } from 'bun:test';
import { mkdtemp, rm } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import Router from '../../src/server/http/route-handler.js';
import FyloBrowser from '../../src/server/fylo-browser/fylo-browser.js';

/** @type {string[]} */
const tempDirs = [];

/** @type {Record<string, string | undefined>} */
const originalEnv = {};

for (const key of ['YON_DATA_BROWSER_ENABLED', 'YON_DATA_BROWSER_READONLY', 'FYLO_ROOT']) {
    originalEnv[key] = process.env[key];
}

beforeEach(async () => {
    Router.resetStaticState();
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-fylo-rest-'));
    tempDirs.push(root);
    process.env.YON_DATA_BROWSER_ENABLED = 'true';
    process.env.YON_DATA_BROWSER_READONLY = 'false';
    process.env.FYLO_ROOT = root;
    FyloBrowser.registerRoutes();
});

afterEach(async () => {
    Router.resetStaticState();
    for (const [key, value] of Object.entries(originalEnv)) {
        if (value === undefined) delete process.env[key];
        else process.env[key] = value;
    }
    await Promise.all(tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true })));
});

/**
 * @param {string} method
 * @param {string} pathname
 * @param {Record<string, unknown>} [body]
 */
async function fyloRequest(method, pathname, body) {
    const handler = Router.reqRoutes['/_fylo/*']?.[method];
    if (!handler) throw new Error(`Missing FYLO REST handler for ${method}`);
    const response = await handler(new Request(`http://localhost${pathname}`, {
        method,
        headers: body ? { 'Content-Type': 'application/json' } : undefined,
        body: body ? JSON.stringify(body) : undefined,
    }));
    const text = await response.text();
    return {
        status: response.status,
        body: text ? JSON.parse(text) : null,
    };
}

/**
 * @param {string} pathname
 */
async function fyloApiGet(pathname) {
    const handler = Router.reqRoutes[pathname.split('?')[0]]?.GET;
    if (!handler) throw new Error(`Missing FYLO API handler for ${pathname}`);
    const response = await handler(new Request(`http://localhost${pathname}`));
    const text = await response.text();
    return {
        status: response.status,
        body: text ? JSON.parse(text) : null,
    };
}

test('serves FYLO documents through Django-style collection and detail URLs', async () => {
    const create = await fyloRequest('POST', '/_fylo/books/', {
        title: 'Tachyon Patterns',
        status: 'draft',
    });
    expect(create.status).toBe(201);
    expect(typeof create.body.id).toBe('string');

    const firstId = String(create.body.id);
    const list = await fyloRequest('GET', '/_fylo/books/?limit=10');
    expect(list.status).toBe(200);
    expect(list.body.collection).toBe('books');
    expect(list.body.docs.some(/** @param {{ id: string }} entry */ (entry) => entry.id === firstId)).toBe(true);

    const detail = await fyloRequest('GET', `/_fylo/books/${firstId}/`);
    expect(detail.status).toBe(200);
    expect(detail.body.doc.title).toBe('Tachyon Patterns');

    const put = await fyloRequest('PUT', `/_fylo/books/${firstId}/`, {
        title: 'Tachyon Patterns',
        status: 'reviewed',
    });
    expect(put.status).toBe(200);
    expect(typeof put.body.id).toBe('string');

    const putId = String(put.body.id);
    const replaced = await fyloRequest('GET', `/_fylo/books/${putId}/`);
    expect(replaced.status).toBe(200);
    expect(replaced.body.doc.status).toBe('reviewed');

    const patch = await fyloRequest('PATCH', `/_fylo/books/${putId}/`, { status: 'published' });
    expect(patch.status).toBe(200);
    expect(typeof patch.body.id).toBe('string');

    const nextId = String(patch.body.id);
    const updated = await fyloRequest('GET', `/_fylo/books/${nextId}/`);
    expect(updated.status).toBe(200);
    expect(updated.body.doc.status).toBe('published');

    const deleted = await fyloRequest('DELETE', `/_fylo/books/${nextId}/`);
    expect(deleted.status).toBe(204);
    expect(deleted.body).toBe(null);
});

test('rejects traversal-shaped collection names before tailing FYLO event files', async () => {
    const response = await fyloApiGet('/_fylo/api/events?collection=..%2Fsecrets');

    expect(response.status).toBe(200);
    expect(response.body.error).toBe('invalid collection query parameter');
});

test('malformed encoded REST collection paths fall through as not found', async () => {
    const response = await fyloRequest('GET', '/_fylo/%E0%A4%A/');

    expect(response.status).toBe(404);
    expect(response.body.error).toBe('not found');
});
