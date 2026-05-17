// @ts-check

import Fylo from '@d31ma/fylo';
import { readFile } from 'fs/promises';
import path from 'path';
import { fyloOptions } from '../../../src/server/fylo-options.js';

export default class FyloTelemetryRepository {
    /**
     * @param {{ root: string, collection?: string }} options
     */
    constructor(options) {
        this.root = options.root;
        this.collection = options.collection ?? 'otel-spans';
        this.fylo = new Fylo(fyloOptions(this.root));
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
                entries.push(/** @type {Record<string, unknown>} */ (doc));
                for (const entry of Object.values(/** @type {Record<string, unknown>} */ (doc))) {
                    if (entry && typeof entry === 'object') {
                        entries.push(/** @type {Record<string, unknown>} */ (entry));
                    }
                }
            }
        } catch {
            return await this.findDocsOnDisk();
        }
        if (entries.length === 0) return await this.findDocsOnDisk();
        return entries;
    }

    /**
     * @returns {Promise<Array<Record<string, unknown>>>}
     */
    async findDocsOnDisk() {
        /** @type {Array<Record<string, unknown>>} */
        const entries = [];
        const docsRoot = path.join(this.root, '.collections', this.collection, 'docs');
        try {
            for await (const relativePath of new Bun.Glob('**/*.json').scan({ cwd: docsRoot })) {
                try {
                    entries.push(JSON.parse(await readFile(path.join(docsRoot, relativePath), 'utf8')));
                } catch {
                    // Ignore documents that are being flushed while the dashboard reads.
                }
            }
        } catch {
            return [];
        }
        return entries;
    }
}
