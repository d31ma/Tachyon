// @ts-check

import Fylo from '@d31ma/fylo';

export default class FyloTelemetryRepository {
    /**
     * @param {{ root: string, collection?: string }} options
     */
    constructor(options) {
        this.root = options.root;
        this.collection = options.collection ?? 'otel-spans';
        this.fylo = new Fylo({ root: this.root });
    }

    /**
     * @returns {Promise<Array<Record<string, unknown>>>}
     */
    async queryFlatSpans() {
        try {
            const result = await this.fylo.executeSQL(`SELECT * FROM ${this.collection} ORDER BY startTimeUnixNano DESC`);
            return Array.isArray(result) ? result.map((doc) => /** @type {Record<string, unknown>} */ (doc)) : [];
        } catch {
            return [];
        }
    }

    /**
     * @returns {Promise<Array<Record<string, unknown>>>}
     */
    async findPersistedEntries() {
        /** @type {Array<Record<string, unknown>>} */
        const entries = [];
        try {
            for await (const doc of this.fylo.findDocs(this.collection, {}).collect()) {
                for (const entry of Object.values(/** @type {Record<string, unknown>} */ (doc))) {
                    if (entry && typeof entry === 'object') {
                        entries.push(/** @type {Record<string, unknown>} */ (entry));
                    }
                }
            }
        } catch {
            return [];
        }
        return entries;
    }
}
