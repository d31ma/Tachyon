// @ts-check
import Fylo, { LocalQueue } from '@d31ma/fylo';
import { fyloOptions } from '../../../../../src/server/fylo-options.js';
import YonRealtime from '../../../../../src/server/realtime/realtime.js';

const CLIENT_ID_PATTERN = /^[0-9A-Z]{11}$/;
const CLIENTS_COLLECTION = 'demo-realtime-clients';
const MESSAGES_COLLECTION = 'demo-realtime-messages';

export default class RealtimeRepository {
    /** @param {{ root?: string }} [options] */
    constructor(options = {}) {
        this.root = options.root || YonRealtime.root();
        this.fylo = new Fylo(this.root, fyloOptions(this.root));
        this.queue = new LocalQueue({ root: this.root });
    }

    /**
     * FYLO exposes collections as dynamic proxy properties; give them a
     * typed doorway so checkJs knows the accessor shape.
     * @param {string} name
     * @returns {{ create(): Promise<void>, put(data: object): Promise<string>, find(query: object): Promise<{ collect(): AsyncIterable<unknown> }> }}
     */
    collection(name) {
        return /** @type {Record<string, any>} */ (/** @type {unknown} */ (this.fylo))[name];
    }

    /**
     * @param {{ nickname: string }} input
     * @returns {Promise<{ clientId: string, nickname: string, streamUrl: string, registeredAt: string }>}
     */
    async registerClient(input) {
        await this.collection(CLIENTS_COLLECTION).create();
        const clientId = await Fylo.uniqueTTID(undefined);
        const registeredAt = new Date().toISOString();
        const client = {
            clientId,
            nickname: input.nickname,
            streamUrl: `${this.realtimePath()}/stream?clientId=${clientId}`,
            registeredAt,
        };
        await this.collection(CLIENTS_COLLECTION).put(client);
        return client;
    }

    /**
     * @returns {Promise<Array<{ clientId: string, nickname: string, streamUrl: string, registeredAt: string }>>}
     */
    async listClients() {
        /** @type {Array<{ clientId: string, nickname: string, streamUrl: string, registeredAt: string }>} */
        const clients = [];
        try {
            const found = await this.collection(CLIENTS_COLLECTION).find({});
            for await (const entry of found.collect()) {
                RealtimeRepository.collectClients(entry, clients);
            }
        }
        catch {
            // No clients registered yet — the collection does not exist until
            // the first registration creates it.
        }
        const unique = new Map(clients.map((client) => [client.clientId, client]));
        return [...unique.values()].sort((a, b) => a.nickname.localeCompare(b.nickname));
    }

    /**
     * @param {{ from: string, to: string, text: string }} input
     * @returns {Promise<{ id: string, to: string, stored: true, event: string, message: { from: string, to: string, text: string, sentAt: string } }>}
     */
    async sendMessage(input) {
        this.assertClientId(input.from, 'from');
        this.assertClientId(input.to, 'to');
        const message = {
            from: input.from,
            to: input.to,
            text: input.text,
            sentAt: new Date().toISOString(),
        };
        const event = 'demo.message';
        const id = await this.queue.publish(this.topic(input.to), {
            to: input.to,
            from: input.from,
            event,
            data: message,
            sentAt: message.sentAt,
        });
        await this.collection(MESSAGES_COLLECTION).create();
        await this.collection(MESSAGES_COLLECTION).put({
            queueMessageId: id,
            event,
            ...message,
        });
        return { id, to: input.to, stored: true, event, message };
    }

    /** @returns {string} */
    realtimePath() {
        return YonRealtime.publicPath();
    }

    /** @param {string} clientId */
    topic(clientId) {
        this.assertClientId(clientId, 'clientId');
        return `yon.realtime.${clientId}`;
    }

    /**
     * @param {string} value
     * @param {string} field
     */
    assertClientId(value, field) {
        if (!CLIENT_ID_PATTERN.test(value)) throw new Error(`${field} must be an 11-character FYLO TTID`);
    }

    /**
     * @param {unknown} value
     * @returns {value is { clientId: string, nickname: string, streamUrl: string, registeredAt: string }}
     */
    static isClient(value) {
        return Boolean(
            value
            && typeof value === 'object'
            && typeof /** @type {{ clientId?: unknown }} */ (value).clientId === 'string'
            && typeof /** @type {{ nickname?: unknown }} */ (value).nickname === 'string'
            && typeof /** @type {{ streamUrl?: unknown }} */ (value).streamUrl === 'string'
            && typeof /** @type {{ registeredAt?: unknown }} */ (value).registeredAt === 'string',
        );
    }

    /**
     * @param {unknown} value
     * @param {Array<{ clientId: string, nickname: string, streamUrl: string, registeredAt: string }>} clients
     */
    static collectClients(value, clients) {
        if (RealtimeRepository.isClient(value)) {
            clients.push(value);
            return;
        }
        if (!value || typeof value !== 'object') return;
        for (const nested of Object.values(/** @type {Record<string, unknown>} */ (value))) {
            RealtimeRepository.collectClients(nested, clients);
        }
    }
}
