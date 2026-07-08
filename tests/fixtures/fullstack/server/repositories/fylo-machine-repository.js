// @ts-check

import { mkdir } from 'fs/promises';
import path from 'path';

export default class FyloMachineRepository {
    /**
     * @param {{ root?: string, schemaDir?: string, executable?: string }} [options]
     */
    constructor(options = {}) {
        this.root = options.root ?? process.env.FYLO_ROOT ?? `${process.cwd()}/db`;
        this.schemaDir = options.schemaDir ?? process.env.FYLO_SCHEMA_DIR ?? process.env.FYLO_SCHEMA ?? `${process.cwd()}/db/schemas`;
        this.executable = options.executable ?? process.env.FYLO_EXEC_PATH ?? process.env.FYLO_BINARY ?? 'fylo';
    }

    /**
     * @param {Record<string, unknown>} request
     * @returns {Promise<unknown>}
     */
    async exec(request) {
        // Fylo is binary-first now: drive the `fylo` binary directly (the old
        // `bunx fylo.exec` npm bin no longer exists).
        const command = [this.executable, 'exec', '--request', '-', '--root', this.root];
        const process = Bun.spawn({
            cmd: command,
            stdin: 'pipe',
            stdout: 'pipe',
            stderr: 'pipe',
        });
        process.stdin.write(JSON.stringify(request));
        process.stdin.end();
        const [stdout, stderr, exitCode] = await Promise.all([
            new Response(process.stdout).text(),
            new Response(process.stderr).text(),
            process.exited,
        ]);
        if (exitCode !== 0) {
            throw new Error(stderr || stdout || 'fylo.exec failed');
        }
        const response = JSON.parse(stdout || '{}');
        if (!response.ok) {
            throw new Error(response.error?.message ?? 'fylo.exec returned an error');
        }
        return response.result;
    }

    /**
     * @param {string} language
     * @param {string} requestId
     * @returns {Promise<{ collection: string, operations: string[], resultCount: number }>}
     */
    async demonstrate(language, requestId) {
        const collection = 'language-route-events';
        await this.exec({ op: 'createCollection', collection });
        const id = await this.exec({
            op: 'putData',
            collection,
            data: { language, requestId, source: 'fylo.exec', kind: 'single' },
        });
        await this.exec({
            op: 'findDocs',
            collection,
            query: { $ops: [{ language: { $eq: language } }], $limit: 10 },
        });
        return { collection, operations: ['createCollection', 'putData', 'findDocs'], resultCount: id ? 1 : 0 };
    }

    /**
     * @param {string} requestId
     * @returns {Promise<{ collection: string, operations: string[], resultCount: number }>}
     */
    async demonstrateAll(requestId) {
        const collection = 'fylo-operation-runs';
        const related = 'fylo-related-records';
        const disposable = 'fylo-disposable-runs';
        // Use a demo-only collection name so ensureDemoSchema() can't clobber
        // the real `items` schema other routes rely on.
        const schemaCollection = 'fylo-demo-items';
        /** @type {string[]} */
        const operations = [];
        /**
         * @param {Record<string, unknown> & { op: string }} request
         * @returns {Promise<unknown>}
         */
        const run = async (request) => {
            const result = await this.exec({ requestId, ...request });
            operations.push(String(request.op));
            return result;
        };

        await run({ op: 'createCollection', collection });
        await run({ op: 'inspectCollection', collection });
        const firstId = String(await run({ op: 'putData', collection, data: { name: 'single', source: 'api', language: 'javascript', requestId } }));
        await run({
            op: 'batchPutData',
            collection,
            batch: [
                { name: 'batch-a', source: 'api', language: 'javascript', requestId, group: 'join' },
                { name: 'batch-b', source: 'api', language: 'javascript', requestId, group: 'join' },
            ],
        });
        await run({ op: 'getDoc', collection, id: firstId });
        await run({ op: 'getLatest', collection, id: firstId });
        await run({ op: 'findDocs', collection, query: { $ops: [{ requestId: { $eq: requestId } }], $limit: 10 } });
        await run({ op: 'executeSQL', sql: `SELECT * FROM ${collection}` });
        await run({
            op: 'patchDoc',
            collection,
            id: firstId,
            newDoc: { name: 'single-patched', source: 'api', language: 'javascript', requestId },
        });
        await run({
            op: 'patchDocs',
            collection,
            update: {
                $set: { source: 'dashboard' },
                $where: { $ops: [{ group: { $eq: 'join' } }] },
            },
        });
        await run({ op: 'createCollection', collection: related });
        await run({ op: 'putData', collection: related, data: { group: 'join', related: 'yes', requestId } });
        await run({
            op: 'joinDocs',
            join: {
                $leftCollection: collection,
                $rightCollection: related,
                $mode: 'inner',
                $on: { group: { $eq: 'group' } },
                $limit: 5,
            },
        });
        const imported = encodeURI(`data:application/json,${JSON.stringify([{ name: 'imported', source: 'api', language: 'javascript', requestId }])}`);
        await run({ op: 'importBulkData', collection, url: imported, limitOrOptions: 1 });
        await this.ensureDemoSchema();
        await run({ op: 'schemaInspect', collection: schemaCollection, schemaDir: this.schemaDir });
        await run({ op: 'schemaCurrent', collection: schemaCollection, schemaDir: this.schemaDir });
        await run({ op: 'schemaHistory', collection: schemaCollection, schemaDir: this.schemaDir });
        await run({ op: 'schemaDoctor', collection: schemaCollection, schemaDir: this.schemaDir });
        await run({ op: 'schemaValidate', collection: schemaCollection, schemaDir: this.schemaDir, document: { name: 'schema', source: 'api', tags: ['demo'], price: '1.00', inStock: 'true' } });
        await run({ op: 'schemaMaterialize', collection: schemaCollection, schemaDir: this.schemaDir, document: { name: 'schema', source: 'api', tags: ['demo'], price: '1.00', inStock: 'true' } });
        await run({ op: 'rebuildCollection', collection });
        await run({ op: 'delDoc', collection, id: firstId });
        await run({ op: 'findDeletedDocs', collection, query: { $deleted: { $gte: 0 } } });
        await run({ op: 'restoreDoc', collection, id: firstId });
        await run({ op: 'delDocs', collection, delete: { $ops: [{ requestId: { $eq: requestId } }] } });
        await run({ op: 'createCollection', collection: disposable });
        await run({ op: 'dropCollection', collection: disposable });

        return { collection, operations, resultCount: operations.length };
    }

    async ensureDemoSchema() {
        // Writes to a demo-only collection so existing schemas (e.g. `items`,
        // `users`, `otel-spans`) are not mutated. Keep collection name in
        // sync with `schemaCollection` in demonstrateAll().
        const collection = 'fylo-demo-items';
        await mkdir(path.join(this.schemaDir, collection, 'history'), { recursive: true });
        await Bun.write(path.join(this.schemaDir, collection, 'manifest.json'), JSON.stringify({
            current: 'v1',
            versions: [{ v: 'v1', addedAt: '2026-04-29T00:00:00.000Z' }],
        }, null, 2));
        await Bun.write(path.join(this.schemaDir, collection, 'history', 'v1.schema.json'), JSON.stringify({
            name: '^.{1,200}$',
            source: '^(api|dashboard|seed)$',
            tags: ['^[a-z0-9][a-z0-9-]{0,63}$'],
            price: '^\\d+(\\.\\d{1,2})?$',
            inStock: '^(true|false)$',
        }, null, 2));
    }
}
