// @ts-check
//
// Minimal durable per-topic message log for YonRealtime.
//
// Fylo 26.28 is binary-first, so its in-process LocalQueue is no longer
// importable. Realtime's queue is an internal pub/sub — not the document DB —
// so it doesn't belong in a fylo collection (which the `fylo` binary owns and
// would conflict on). This is a self-contained append-only NDJSON log per topic
// under a Tachyon-owned directory, preserving the exact interface YonRealtime
// used (`publish` / `readTopic`) and its integer-position SSE cursor protocol.

import { mkdir, appendFile, readFile } from 'node:fs/promises';
import path from 'node:path';

/** @typedef {{ event?: unknown, [k: string]: unknown }} TopicPayload */
/** @typedef {{ message: { id: string, payload: TopicPayload }, nextPosition: number }} TopicRecord */

/** @param {string} value */
function safeSegment(value) {
    return encodeURIComponent(value).replace(/%/g, '_');
}

export default class TopicLog {
    /** @param {string} root Directory that holds the per-topic log files. */
    constructor(root) {
        this.dir = path.join(root, '.yon-realtime');
        /** @type {Promise<void> | null} */
        this.ready = null;
    }

    async ensureDir() {
        if (!this.ready) this.ready = mkdir(this.dir, { recursive: true }).then(() => undefined);
        await this.ready;
    }

    /** @param {string} topic */
    fileFor(topic) {
        return path.join(this.dir, `${safeSegment(topic)}.ndjson`);
    }

    /**
     * Append a message to a topic. Returns the new message id.
     * @param {string} topic
     * @param {TopicPayload} payload
     * @returns {Promise<string>}
     */
    async publish(topic, payload) {
        await this.ensureDir();
        const id = crypto.randomUUID();
        await appendFile(this.fileFor(topic), `${JSON.stringify({ id, payload })}\n`, 'utf8');
        return id;
    }

    /**
     * Read up to `limit` records after integer `position` (a line offset).
     * ponytail: reads the whole topic file per poll — fine for modest realtime
     * volumes; add rotation/capping if a single topic grows unbounded.
     * @param {string} topic
     * @param {number} position
     * @param {number} limit
     * @returns {Promise<TopicRecord[]>}
     */
    async readTopic(topic, position, limit) {
        let text;
        try {
            text = await readFile(this.fileFor(topic), 'utf8');
        } catch {
            return []; // no messages published to this topic yet
        }
        const lines = text.split('\n');
        /** @type {TopicRecord[]} */
        const records = [];
        for (let i = position; i < lines.length && records.length < limit; i++) {
            const line = lines[i]?.trim();
            if (!line) continue;
            try {
                const { id, payload } = JSON.parse(line);
                records.push({ message: { id, payload }, nextPosition: i + 1 });
            } catch {
                // Skip a torn/partial line rather than break the stream.
            }
        }
        return records;
    }
}
