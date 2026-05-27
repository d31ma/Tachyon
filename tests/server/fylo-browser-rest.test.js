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

test('filters collection documents using PostgREST eq operator', async () => {
    await fyloRequest('POST', '/_fylo/people/', { name: 'Alice', role: 'admin' });
    await fyloRequest('POST', '/_fylo/people/', { name: 'Bob', role: 'viewer' });
    await fyloRequest('POST', '/_fylo/people/', { name: 'Carol', role: 'admin' });

    const result = await fyloRequest('GET', '/_fylo/people/?role=eq.admin');
    expect(result.status).toBe(200);
    expect(result.body.docs.length).toBe(2);
    const names = result.body.docs.map(/** @param {{ doc: { name: string } }} d */ (d) => d.doc.name).sort();
    expect(names).toEqual(['Alice', 'Carol']);
});

test('filters with multiple PostgREST eq conditions (implicit AND)', async () => {
    await fyloRequest('POST', '/_fylo/staff/', { name: 'Alice', dept: 'eng', level: 'senior' });
    await fyloRequest('POST', '/_fylo/staff/', { name: 'Bob', dept: 'eng', level: 'junior' });
    await fyloRequest('POST', '/_fylo/staff/', { name: 'Carol', dept: 'sales', level: 'senior' });

    const result = await fyloRequest('GET', '/_fylo/staff/?dept=eq.eng&level=eq.senior');
    expect(result.status).toBe(200);
    expect(result.body.docs.length).toBe(1);
    expect(/** @type {{ name: string }} */ (result.body.docs[0].doc).name).toBe('Alice');
});

test('select parameter returns only requested fields', async () => {
    await fyloRequest('POST', '/_fylo/profiles/', { name: 'Alice', email: 'a@b.com', role: 'admin', age: '30' });

    const result = await fyloRequest('GET', '/_fylo/profiles/?select=name,role');
    expect(result.status).toBe(200);
    expect(result.body.docs.length).toBeGreaterThan(0);
    const doc = /** @type {Record<string, unknown>} */ (result.body.docs[0].doc);
    expect(doc.name).toBe('Alice');
    expect(doc.role).toBe('admin');
    expect(doc.email).toBeUndefined();
    expect(doc.age).toBeUndefined();
});

test('order parameter sorts results', async () => {
    await fyloRequest('POST', '/_fylo/scores/', { player: 'Alice', score: '30' });
    await fyloRequest('POST', '/_fylo/scores/', { player: 'Bob', score: '10' });
    await fyloRequest('POST', '/_fylo/scores/', { player: 'Carol', score: '20' });

    const asc = await fyloRequest('GET', '/_fylo/scores/?order=score.asc');
    expect(asc.status).toBe(200);
    const ascScores = asc.body.docs.map(/** @param {{ doc: { score: string } }} d */ (d) => d.doc.score);
    expect(ascScores).toEqual(['10', '20', '30']);

    const desc = await fyloRequest('GET', '/_fylo/scores/?order=score.desc');
    expect(desc.status).toBe(200);
    const descScores = desc.body.docs.map(/** @param {{ doc: { score: string } }} d */ (d) => d.doc.score);
    expect(descScores).toEqual(['30', '20', '10']);
});

test('offset and limit paginate results', async () => {
    for (let i = 1; i <= 5; i++) {
        await fyloRequest('POST', '/_fylo/pages/', { seq: String(i) });
    }

    const page1 = await fyloRequest('GET', '/_fylo/pages/?limit=2&offset=0');
    expect(page1.status).toBe(200);
    expect(page1.body.docs.length).toBe(2);

    const page2 = await fyloRequest('GET', '/_fylo/pages/?limit=2&offset=2');
    expect(page2.status).toBe(200);
    expect(page2.body.docs.length).toBe(2);

    const page1Ids = new Set(page1.body.docs.map(/** @param {{ id: string }} d */ (d) => d.id));
    const page2Ids = new Set(page2.body.docs.map(/** @param {{ id: string }} d */ (d) => d.id));
    // Pages must not overlap
    for (const id of page2Ids) {
        expect(page1Ids.has(id)).toBe(false);
    }
});

test('bare values without PostgREST operator are ignored as filters', async () => {
    await fyloRequest('POST', '/_fylo/tags/', { label: 'important' });

    // "important" is not a valid PostgREST filter (missing operator.value format)
    const result = await fyloRequest('GET', '/_fylo/tags/?label=important');
    expect(result.status).toBe(200);
    // All docs returned because the bare value is not parsed as a filter
    expect(result.body.docs.length).toBe(1);
});

test('malformed encoded REST collection paths fall through as not found', async () => {
    const response = await fyloRequest('GET', '/_fylo/%E0%A4%A/');

    expect(response.status).toBe(404);
    expect(response.body.error).toBe('not found');
});
