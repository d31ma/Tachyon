// @ts-check
import Fylo, { LocalQueue } from '@d31ma/fylo';
import { fyloOptions } from '../../../src/server/fylo-options.js';
import YonRealtime from '../../../src/server/realtime/realtime.js';

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

    async ensureCollections() {
        await Promise.all([
            this.createCollection(CLIENTS_COLLECTION),
            this.createCollection(MESSAGES_COLLECTION),
        ]);
    }

    /** @param {string} collection */
    async createCollection(collection) {
        try {
            await this.fylo.createCollection(collection);
        }
        catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            if (!message.toLowerCase().includes('exist')) throw error;
        }
    }

    /**
     * @param {{ nickname: string }} input
     * @returns {Promise<{ clientId: string, nickname: string, streamUrl: string, registeredAt: string }>}
     */
    async registerClient(input) {
        await this.ensureCollections();
        const clientId = await Fylo.uniqueTTID(undefined);
        const registeredAt = new Date().toISOString();
        const client = {
            clientId,
            nickname: input.nickname,
            streamUrl: `${this.realtimePath()}/stream?clientId=${clientId}`,
            registeredAt,
        };
        await this.fylo.putData(CLIENTS_COLLECTION, client);
        return client;
    }

    /**
     * @returns {Promise<Array<{ clientId: string, nickname: string, streamUrl: string, registeredAt: string }>>}
     */
    async listClients() {
        await this.ensureCollections();
        /** @type {Array<{ clientId: string, nickname: string, streamUrl: string, registeredAt: string }>} */
        const clients = [];
        for await (const entry of this.fylo.findDocs(CLIENTS_COLLECTION, {}).collect()) {
            RealtimeRepository.collectClients(entry, clients);
        }
        return clients.sort((a, b) => a.nickname.localeCompare(b.nickname));
    }

    /**
     * @param {{ from: string, to: string, text: string }} input
     * @returns {Promise<{ id: string, to: string, stored: true, event: string, message: { from: string, to: string, text: string, sentAt: string } }>}
     */
    async sendMessage(input) {
        await this.ensureCollections();
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
        await this.fylo.putData(MESSAGES_COLLECTION, {
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
