// @ts-check
import { afterEach, beforeEach, expect, test } from 'bun:test';
import { mkdtemp, rm } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import Router from '../../src/server/http/route-handler.js';
import YonRealtime from '../../src/server/realtime/realtime.js';
import RealtimeRepository from '../fixtures/fullstack/server/repositories/realtime-repository.js';

/** @type {Record<string, string | undefined>} */
const originalEnv = {};

for (const key of ['YON_REALTIME_ENABLED', 'YON_REALTIME_ROOT', 'YON_REALTIME_POLL_MS', 'YON_REALTIME_HEARTBEAT_MS', 'FYLO_ROOT']) {
    originalEnv[key] = process.env[key];
}

/** @type {string[]} */
const tempDirs = [];

beforeEach(async () => {
    Router.resetStaticState();
    YonRealtime.connections.clear();
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-realtime-'));
    tempDirs.push(root);
    process.env.YON_REALTIME_ENABLED = 'true';
    process.env.YON_REALTIME_ROOT = root;
    process.env.YON_REALTIME_POLL_MS = '100';
    process.env.YON_REALTIME_HEARTBEAT_MS = '1000';
    YonRealtime.registerRoutes();
});

afterEach(async () => {
    Router.resetStaticState();
    YonRealtime.connections.clear();
    for (const [key, value] of Object.entries(originalEnv)) {
        if (value === undefined) delete process.env[key];
        else process.env[key] = value;
    }
    await Promise.all(tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true })));
});

/**
 * @param {string} pathname
 * @param {Record<string, unknown>} [body]
 */
async function post(pathname, body = {}) {
    const handler = Router.reqRoutes[pathname]?.POST;
    if (!handler) throw new Error(`Missing POST handler for ${pathname}`);
    const response = await handler(new Request(`http://localhost${pathname}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
    }));
    return {
        status: response.status,
        body: await response.json(),
    };
}

/** @param {ReadableStream<Uint8Array> | null} body @param {string} expected */
async function readUntil(body, expected) {
    if (!body) throw new Error('Expected response body');
    const reader = body.getReader();
    let text = '';
    const deadline = Date.now() + 2000;
    while (!text.includes(expected) && Date.now() < deadline) {
        const result = await reader.read();
        if (result.done) break;
        text += new TextDecoder().decode(result.value);
    }
    await reader.cancel();
    return text;
}

test('registers a durable TTID client identifier', async () => {
    const response = await post('/_yon/realtime/clients');

    expect(response.status).toBe(201);
    expect(response.body.clientId).toMatch(/^[0-9A-Z]{11}$/);
    expect(response.body.cursor).toBe(0);
});

test('stores offline messages and replays them when the client reconnects', async () => {
    const client = await post('/_yon/realtime/clients');
    const clientId = String(client.body.clientId);

    const publish = await post('/_yon/realtime/messages', {
        to: clientId,
        event: 'chat.message',
        data: { text: 'hello from storage' },
    });

    expect(publish.status).toBe(202);
    expect(publish.body.delivered).toBe(0);
    expect(publish.body.stored).toBe(true);

    const handler = Router.reqRoutes['/_yon/realtime/stream']?.GET;
    if (!handler) throw new Error('Missing realtime stream handler');
    const response = await handler(new Request(`http://localhost/_yon/realtime/stream?clientId=${clientId}`, {
        headers: { Accept: 'text/event-stream' },
    }));
    const chunk = await readUntil(response.body, 'hello from storage');

    expect(response.status).toBe(200);
    expect(response.headers.get('Content-Type')).toContain('text/event-stream');
    expect(chunk).toContain('event: ready');
    expect(chunk).toContain('event: chat.message');
    expect(chunk).toContain('hello from storage');
});

test('reports active SSE delivery when the target client is connected', async () => {
    const client = await post('/_yon/realtime/clients');
    const clientId = String(client.body.clientId);
    const handler = Router.reqRoutes['/_yon/realtime/stream']?.GET;
    if (!handler) throw new Error('Missing realtime stream handler');

    const response = await handler(new Request(`http://localhost/_yon/realtime/stream?clientId=${clientId}`, {
        headers: { Accept: 'text/event-stream' },
    }));
    const reader = response.body?.getReader();
    if (!reader) throw new Error('Expected stream reader');
    await reader.read();

    const publish = await post('/_yon/realtime/messages', {
        to: clientId,
        data: { text: 'online' },
    });
    let chunk = '';
    const deadline = Date.now() + 2000;
    while (!chunk.includes('online') && Date.now() < deadline) {
        const delivered = await reader.read();
        if (delivered.done) break;
        chunk += new TextDecoder().decode(delivered.value);
    }
    await reader.cancel();

    expect(publish.status).toBe(202);
    expect(publish.body.delivered).toBe(1);
    expect(chunk).toContain('online');
});

test('rejects unsafe client identifiers', async () => {
    const publish = await post('/_yon/realtime/messages', {
        to: '../../secrets',
        data: { text: 'nope' },
    });

    expect(publish.status).toBe(400);
    expect(publish.body.detail).toContain('clientId');
});

test('rejects unsafe event names', async () => {
    const client = await post('/_yon/realtime/clients');
    const publish = await post('/_yon/realtime/messages', {
        to: String(client.body.clientId),
        event: '1 bad event',
        data: { text: 'nope' },
    });

    expect(publish.status).toBe(400);
    expect(publish.body.detail).toContain('event');
});

test('example repository shares the core realtime root when YON_REALTIME_ROOT is empty', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-realtime-shared-'));
    tempDirs.push(root);
    process.env.YON_REALTIME_ROOT = '';
    process.env.FYLO_ROOT = root;
    Router.resetStaticState();
    YonRealtime.connections.clear();
    YonRealtime.registerRoutes();

    const repository = new RealtimeRepository();
    const ada = await repository.registerClient({ nickname: 'Ada' });
    const grace = await repository.registerClient({ nickname: 'Grace' });
    const clients = await repository.listClients();

    expect(repository.root).toBe(root);
    expect(YonRealtime.root()).toBe(root);
    expect(clients.map(client => client.nickname).sort()).toEqual(['Ada', 'Grace']);

    const handler = Router.reqRoutes['/_yon/realtime/stream']?.GET;
    if (!handler) throw new Error('Missing realtime stream handler');
    const response = await handler(new Request(`http://localhost/_yon/realtime/stream?clientId=${ada.clientId}&cursor=0`, {
        headers: { Accept: 'text/event-stream' },
    }));
    const reader = response.body?.getReader();
    if (!reader) throw new Error('Expected stream reader');
    await reader.read();

    const sent = await repository.sendMessage({
        from: grace.clientId,
        to: ada.clientId,
        text: 'hi Ada',
    });
    expect(sent.stored).toBe(true);

    let chunk = '';
    const deadline = Date.now() + 2000;
    while (!chunk.includes('hi Ada') && Date.now() < deadline) {
        const delivered = await reader.read();
        if (delivered.done) break;
        chunk += new TextDecoder().decode(delivered.value);
    }
    await reader.cancel();

    expect(chunk).toContain('event: demo.message');
    expect(chunk).toContain('hi Ada');
    expect(chunk).toContain(grace.clientId);
});
