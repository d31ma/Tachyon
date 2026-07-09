// @ts-check

import { Fylo } from '../../../../../src/vendor/fylo/fylo-node.mjs';
import { readFile } from 'fs/promises';
import path from 'path';
import { fyloOptions } from '../../../../../src/server/fylo-options.js';

export default class FyloTelemetryRepository {
    /**
     * @param {{ root: string, collection?: string }} options
     */
    constructor(options) {
        this.root = options.root;
        this.collection = options.collection ?? 'otel-spans';
        this.fylo = new Fylo(this.root, fyloOptions(this.root));
    }

    /**
     * Spans are stored as OTLP JSON envelopes rather than flat rows, so the
     * SQL projection has nothing to add — the document reads below are the
     * authoritative source.
     * @returns {Promise<Array<Record<string, unknown>>>}
     */
    async queryFlatSpans() {
        return [];
    }

    /**
     * @returns {Promise<Array<Record<string, unknown>>>}
     */
    async findPersistedEntries() {
        /** @type {Array<Record<string, unknown>>} */
        const entries = [];
        try {
            const collection = /** @type {Record<string, { find(query: object): Promise<Record<string, unknown>> }>} */ (
                /** @type {unknown} */ (this.fylo)
            )[this.collection];
            // The Node shim's find() resolves to an `{ id: doc }` map.
            const found = /** @type {Record<string, unknown>} */ (await collection.find({}) ?? {});
            for (const doc of Object.values(found)) {
                if (!doc || typeof doc !== 'object') continue;
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
