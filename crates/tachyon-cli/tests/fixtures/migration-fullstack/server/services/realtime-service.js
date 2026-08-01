// @ts-check
import RealtimeRepository from '../repositories/realtime-repository.js';

export default class RealtimeService {
    /** @param {{ repository?: RealtimeRepository }} [options] */
    constructor(options = {}) {
        this.repository = options.repository ?? new RealtimeRepository();
    }

    async listClients() {
        const clients = await this.repository.listClients();
        return {
            clients,
            count: clients.length,
            streamPath: this.repository.realtimePath(),
        };
    }

    /** @param {unknown} body */
    async registerClient(body) {
        const nickname = this.readString(body, 'nickname', 'Browser guest');
        if (nickname.length > 40) return { detail: 'nickname must be 40 characters or fewer' };
        return await this.repository.registerClient({ nickname });
    }

    /** @param {unknown} body */
    async sendMessage(body) {
        if (!body || typeof body !== 'object') return { detail: 'message body is required' };
        const from = this.readString(body, 'from');
        const to = this.readString(body, 'to');
        const text = this.readString(body, 'text');
        if (!from || !to || !text) return { detail: 'from, to, and text are required' };
        if (text.length > 500) return { detail: 'text must be 500 characters or fewer' };
        try {
            return await this.repository.sendMessage({ from, to, text });
        }
        catch (error) {
            return { detail: error instanceof Error ? error.message : String(error) };
        }
    }

    /**
     * @param {unknown} body
     * @param {string} field
     * @param {string} [fallback]
     */
    readString(body, field, fallback = '') {
        if (!body || typeof body !== 'object') return fallback;
        const value = /** @type {Record<string, unknown>} */ (body)[field];
        return typeof value === 'string' ? value.trim() : fallback;
    }
}
