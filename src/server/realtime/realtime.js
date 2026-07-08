// @ts-check
import { Fylo } from '../../vendor/fylo/fylo-node.mjs';
import TTID from '../../vendor/ttid/ttid.mjs';
import path from 'path';
import Router from '../http/route-handler.js';
import { fyloOptions } from '../fylo-options.js';
import TopicLog from './topic-log.js';

const DEFAULT_PATH = '/_yon/realtime';
const CLIENTS_COLLECTION = 'yon-realtime-clients';
const CLIENT_ID_PATTERN = /^[0-9A-Z]{11}$/;
const EVENT_NAME_PATTERN = /^[A-Za-z][A-Za-z0-9_.:-]{0,63}$/;

/** @typedef {{ controller: ReadableStreamDefaultController<Uint8Array>, position: number, lastHeartbeat: number, draining: boolean, closed: boolean }} YonRealtimeConnection */
/** @typedef {{ clientId: string, cursor: number }} YonRealtimeClient */
/** @typedef {(request?: Request, server?: import('bun').Server<any>) => Promise<Response> | Response} RealtimeHandler */
/** @typedef {(handler: RealtimeHandler, options?: { route?: string }) => RealtimeHandler} RealtimeRouteWrapper */

export default class YonRealtime {
    /** @type {Map<string, Set<YonRealtimeConnection>>} */
    static connections = new Map();

    /** @returns {boolean} */
    static enabled() {
        return ['1', 'true', 'yes', 'on'].includes((process.env.YON_REALTIME_ENABLED ?? '').toLowerCase());
    }

    /** @returns {string} */
    static publicPath() {
        const configured = process.env.YON_REALTIME_PATH?.trim();
        if (!configured) return DEFAULT_PATH;
        const normalized = configured.startsWith('/') ? configured.replace(/\/+$/, '') : `/${configured.replace(/\/+$/, '')}`;
        return normalized || DEFAULT_PATH;
    }

    /** @returns {string} */
    static root() {
        return process.env.YON_REALTIME_ROOT
            || process.env.FYLO_ROOT
            || path.join(process.cwd(), '.fylo-data');
    }

    /** @returns {number} */
    static pollIntervalMs() {
        const value = Number(process.env.YON_REALTIME_POLL_MS ?? 1000);
        return Number.isFinite(value) && value >= 100 ? value : 1000;
    }

    /** @returns {number} */
    static heartbeatMs() {
        const value = Number(process.env.YON_REALTIME_HEARTBEAT_MS ?? 15000);
        return Number.isFinite(value) && value >= 1000 ? value : 15000;
    }

    /** @returns {number} */
    static maxEventBytes() {
        const value = Number(process.env.YON_REALTIME_MAX_EVENT_BYTES ?? 65536);
        return Number.isFinite(value) && value >= 1024 ? value : 65536;
    }

    /** @returns {Fylo & Record<string, any>} */
    static db() {
        const root = YonRealtime.root();
        return /** @type {Fylo & Record<string, any>} */ (new Fylo(root, fyloOptions(root)));
    }

    /** @returns {TopicLog} */
    static queue() {
        return new TopicLog(YonRealtime.root());
    }

    /**
     * Mint an 11-char TTID client id matching {@link CLIENT_ID_PATTERN}. TTID is
     * pure computation, so the vendored web generator works server-side with no
     * binary — a real time-ordered id, not a random placeholder.
     * @returns {string}
     */
    static generateClientId() {
        return TTID.generate();
    }

    /** @param {string} clientId */
    static assertClientId(clientId) {
        if (!CLIENT_ID_PATTERN.test(clientId)) {
            throw new Response(JSON.stringify({ detail: 'clientId must be an 11-character FYLO TTID' }), {
                status: 400,
                headers: { 'Content-Type': 'application/json; charset=utf-8' },
            });
        }
    }

    /** @param {unknown} event */
    static assertEventName(event) {
        if (event === undefined || event === null || event === '') return 'message';
        if (typeof event === 'string' && EVENT_NAME_PATTERN.test(event)) return event;
        throw new Response(JSON.stringify({ detail: 'event must match /^[A-Za-z][A-Za-z0-9_.:-]{0,63}$/' }), {
            status: 400,
            headers: { 'Content-Type': 'application/json; charset=utf-8' },
        });
    }

    /** @param {string | null} value */
    static parseCursor(value) {
        if (!value) return 0;
        const cursor = Number(value);
        return Number.isSafeInteger(cursor) && cursor >= 0 ? cursor : 0;
    }

    /** @param {unknown} value */
    static assertRecord(value) {
        if (!value || typeof value !== 'object' || Array.isArray(value)) {
            throw new Response(JSON.stringify({ detail: 'JSON object body required' }), {
                status: 400,
                headers: { 'Content-Type': 'application/json; charset=utf-8' },
            });
        }
        return /** @type {Record<string, unknown>} */ (value);
    }

    /**
     * @param {Request} request
     * @param {{ optional?: boolean }} [options]
     */
    static async readJsonObject(request, options = {}) {
        if (options.optional && !request.body) return {};
        try {
            return YonRealtime.assertRecord(await request.json());
        }
        catch (error) {
            if (error instanceof Response) throw error;
            throw new Response(JSON.stringify({ detail: 'Valid JSON object body required' }), {
                status: 400,
                headers: { 'Content-Type': 'application/json; charset=utf-8' },
            });
        }
    }

    /**
     * @param {unknown} error
     * @param {Request} request
     */
    static errorResponse(error, request) {
        if (error instanceof Response) {
            const headers = new Headers(error.headers);
            for (const [name, value] of Object.entries(Router.getHeaders(request))) headers.set(name, value);
            return new Response(error.body, { status: error.status, statusText: error.statusText, headers });
        }
        return Response.json({ detail: error instanceof Error ? error.message : String(error) }, {
            status: 500,
            headers: Router.getHeaders(request),
        });
    }

    static async ensureCollections() {
        const db = YonRealtime.db();
        try {
            await db[CLIENTS_COLLECTION].create();
        }
        catch (error) {
            if (!String(error instanceof Error ? error.message : error).toLowerCase().includes('exist')) throw error;
        }
    }

    /**
     * @param {string | undefined} requestedClientId
     * @returns {Promise<YonRealtimeClient>}
     */
    static async registerClient(requestedClientId) {
        const clientId = requestedClientId?.trim() || YonRealtime.generateClientId();
        YonRealtime.assertClientId(clientId);
        await YonRealtime.ensureCollections();
        await YonRealtime.db()[CLIENTS_COLLECTION].put({
            [clientId]: {
                clientId,
                registeredAt: new Date().toISOString(),
                updatedAt: new Date().toISOString(),
            },
        });
        return { clientId, cursor: 0 };
    }

    /** @param {Request} request */
    static async registerClientRoute(request) {
        try {
            const body = await YonRealtime.readJsonObject(request, { optional: true });
            const requestedClientId = typeof body.clientId === 'string' ? body.clientId : undefined;
            const client = await YonRealtime.registerClient(requestedClientId);
            return Response.json(client, { status: 201, headers: Router.getHeaders(request) });
        }
        catch (error) {
            return YonRealtime.errorResponse(error, request);
        }
    }

    /** @param {string} clientId */
    static topic(clientId) {
        YonRealtime.assertClientId(clientId);
        return `yon.realtime.${clientId}`;
    }

    /**
     * @param {string} event
     * @param {unknown} data
     * @param {number} id
     */
    static sse(event, data, id) {
        const payload = typeof data === 'string' ? data : JSON.stringify(data);
        return `id: ${id}\nevent: ${event}\ndata: ${payload}\n\n`;
    }

    /** @param {ReadableStreamDefaultController<Uint8Array>} controller @param {string} chunk */
    static enqueue(controller, chunk) {
        controller.enqueue(new TextEncoder().encode(chunk));
    }

    /**
     * @param {string} clientId
     * @param {YonRealtimeConnection} connection
     */
    static async drain(clientId, connection) {
        if (connection.closed || connection.draining) return;
        connection.draining = true;
        try {
            const records = await YonRealtime.queue().readTopic(YonRealtime.topic(clientId), connection.position, 100);
            for (const record of records) {
                if (connection.closed) return;
                YonRealtime.enqueue(
                    connection.controller,
                    YonRealtime.sse(
                        String(record.message.payload.event ?? 'message'),
                        record.message.payload,
                        record.nextPosition,
                    ),
                );
                connection.position = record.nextPosition;
            }
            if (Date.now() - connection.lastHeartbeat >= YonRealtime.heartbeatMs()) {
                YonRealtime.enqueue(connection.controller, `: heartbeat ${Date.now()}\n\n`);
                connection.lastHeartbeat = Date.now();
            }
        }
        finally {
            connection.draining = false;
        }
    }

    /**
     * @param {string} clientId
     * @param {YonRealtimeConnection} connection
     */
    static addConnection(clientId, connection) {
        const set = YonRealtime.connections.get(clientId) ?? new Set();
        set.add(connection);
        YonRealtime.connections.set(clientId, set);
    }

    /**
     * @param {string} clientId
     * @param {YonRealtimeConnection} connection
     */
    static removeConnection(clientId, connection) {
        const set = YonRealtime.connections.get(clientId);
        if (!set) return;
        set.delete(connection);
        if (set.size === 0) YonRealtime.connections.delete(clientId);
    }

    /** @param {Request} request */
    static streamRoute(request) {
        const url = new URL(request.url);
        const clientId = url.searchParams.get('clientId') ?? request.headers.get('x-yon-client-id') ?? '';
        try {
            YonRealtime.assertClientId(clientId);
        }
        catch (error) {
            return YonRealtime.errorResponse(error, request);
        }
        const cursor = YonRealtime.parseCursor(url.searchParams.get('cursor') ?? request.headers.get('last-event-id'));
        /** @type {YonRealtimeConnection | undefined} */
        let activeConnection;
        /** @type {Timer | undefined} */
        let activeInterval;
        const stream = new ReadableStream({
            start(controller) {
                /** @type {YonRealtimeConnection} */
                const connection = {
                    controller,
                    position: cursor,
                    lastHeartbeat: Date.now(),
                    draining: false,
                    closed: false,
                };
                activeConnection = connection;
                YonRealtime.addConnection(clientId, connection);
                YonRealtime.registerClient(clientId)
                    .then(() => {
                        if (connection.closed) return undefined;
                        YonRealtime.enqueue(controller, `event: ready\ndata: ${JSON.stringify({ clientId, cursor })}\n\n`);
                        return YonRealtime.drain(clientId, connection);
                    })
                    .catch(() => {
                        connection.closed = true;
                        try { controller.close(); } catch {}
                        YonRealtime.removeConnection(clientId, connection);
                    });
                const interval = setInterval(() => {
                    YonRealtime.drain(clientId, connection).catch(() => {
                        connection.closed = true;
                        clearInterval(interval);
                        controller.close();
                        YonRealtime.removeConnection(clientId, connection);
                    });
                }, YonRealtime.pollIntervalMs());
                activeInterval = interval;
            },
            cancel() {
                if (activeInterval) clearInterval(activeInterval);
                if (activeConnection) {
                    activeConnection.closed = true;
                    YonRealtime.removeConnection(clientId, activeConnection);
                }
            },
        });
        return new Response(stream, {
            status: 200,
            headers: {
                ...Router.getHeaders(request),
                'Content-Type': 'text/event-stream',
                'Cache-Control': 'no-cache, no-transform',
                Connection: 'keep-alive',
            },
        });
    }

    /** @param {Request} request */
    static async publishRoute(request) {
        try {
            const body = await YonRealtime.readJsonObject(request);
            const to = typeof body.to === 'string' ? body.to.trim() : '';
            YonRealtime.assertClientId(to);
            const event = YonRealtime.assertEventName(body.event);
            const payload = {
                to,
                from: typeof body.from === 'string' ? body.from.slice(0, 128) : undefined,
                event,
                data: body.data ?? null,
                sentAt: new Date().toISOString(),
            };
            const encoded = JSON.stringify(payload);
            if (new TextEncoder().encode(encoded).byteLength > YonRealtime.maxEventBytes()) {
                return Response.json({ detail: 'Realtime event exceeds YON_REALTIME_MAX_EVENT_BYTES' }, {
                    status: 413,
                    headers: Router.getHeaders(request),
                });
            }
            const messageId = await YonRealtime.queue().publish(YonRealtime.topic(to), payload);
            const connections = YonRealtime.connections.get(to) ?? new Set();
            await Promise.all([...connections].map(connection => YonRealtime.drain(to, connection)));
            return Response.json({
                id: messageId,
                to,
                delivered: connections.size,
                stored: true,
            }, { status: 202, headers: Router.getHeaders(request) });
        }
        catch (error) {
            return YonRealtime.errorResponse(error, request);
        }
    }

    /** @param {RealtimeRouteWrapper} [wrapRoute] */
    static registerRoutes(wrapRoute) {
        if (!YonRealtime.enabled()) return;
        const base = YonRealtime.publicPath();
        const wrap = wrapRoute
            ? /** @param {RealtimeHandler} handler @param {string} route */ (handler, route) => wrapRoute(handler, { route })
            : /** @param {RealtimeHandler} handler */ (handler) => handler;

        Router.reqRoutes[`${base}/clients`] = {
            POST: wrap((request = new Request(`http://localhost${base}/clients`, { method: 'POST' })) => YonRealtime.registerClientRoute(request), `${base}/clients`),
        };
        Router.reqRoutes[`${base}/stream`] = {
            GET: wrap((request = new Request(`http://localhost${base}/stream`)) => YonRealtime.streamRoute(request), `${base}/stream`),
        };
        Router.reqRoutes[`${base}/messages`] = {
            POST: wrap((request = new Request(`http://localhost${base}/messages`, { method: 'POST' })) => YonRealtime.publishRoute(request), `${base}/messages`),
        };
    }
}
