// @ts-check
import { CHEX } from '../../vendor/chex/chex.mjs';
import { warm } from '../../vendor/chex/warm.js';
import { mkdtempSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import path from 'path';
import Router from "./route-handler.js";

/** @typedef {Record<string, any>} SchemaRecord */
/** @typedef {Record<string, SchemaRecord>} MethodSchema */

export default class Validate {
    /**
     * CHEX is binary-first now (no importable package). We drive the `chex`
     * binary through the vendored NDJSON shim, wrapped in `warm()` so the
     * long-lived subprocess stays unref'd while idle and never blocks exit.
     * @type {(import('../../vendor/chex/chex.mjs').CHEX) | null}
     */
    static chexClient = null;

    /** Temp dir holding schemas written for the binary (it reads from disk). @type {string | null} */
    static chexSchemaDir = null;

    /** Schema-cache keys already written to {@link chexSchemaDir}. @type {Set<string>} */
    static writtenSchemas = new Set();

    /** @returns {import('../../vendor/chex/chex.mjs').CHEX} */
    static getChex() {
        if (!Validate.chexClient) Validate.chexClient = warm(new CHEX());
        return Validate.chexClient;
    }

    /** @returns {string} */
    static ensureSchemaDir() {
        if (!Validate.chexSchemaDir) Validate.chexSchemaDir = mkdtempSync(path.join(tmpdir(), 'tachyon-chex-'));
        return Validate.chexSchemaDir;
    }

    /**
     * Derives the route directory from a handler file path by stripping the
     * trailing `yon.<ext>` filename. The remaining path segments form the
     * route directory that maps to the registered route pattern.
     * @param {string} handlerPath - Normalized absolute handler file path
     * @returns {string} The relative route pattern (e.g. `/languages/javascript`)
     */
    static routeFromHandlerPath(handlerPath) {
        const normalized = handlerPath.replaceAll('\\', '/');
        const normalizedRoutesPath = Router.routesPath.replaceAll('\\', '/');
        const parts = normalized.split('/');
        parts.pop(); // strip yon.<ext>
        const absoluteDir = parts.join('/');
        return Router.filesystemPathToRoute(absoluteDir.replace(normalizedRoutesPath, '') || '/');
    }

    /**
     * @param {SchemaRecord} data
     * @param {SchemaRecord} schema
     * @param {string} route
     * @param {string} parentRoute
     * @param {string | undefined} method
     * @param {string} io
     */
    static async validateWithChex(data, schema, route, parentRoute, method, io) {
        // The chex binary resolves a schema name against a directory
        // (`<dir>/<name>.schema.json`), so write the schema to the temp dir. Key
        // the file by schema CONTENT (not route/method/io) — the same route can
        // be validated against different schemas (e.g. per status code), so a
        // route-keyed cache would serve a stale schema.
        const serialized = JSON.stringify(schema);
        const key = `tachyon_${new Bun.CryptoHasher('sha256').update(serialized).digest('hex').slice(0, 32)}`;
        const dir = Validate.ensureSchemaDir();
        if (!Validate.writtenSchemas.has(key)) {
            writeFileSync(path.join(dir, `${key}.schema.json`), serialized);
            Validate.writtenSchemas.add(key);
        }
        // Resolves with the validated data, rejects (ValidationError) on mismatch.
        await Validate.getChex().validate(key, data, dir);
    }

    /**
     * @param {MethodSchema | undefined} methodSchema
     * @param {string} io
     * @returns {SchemaRecord | undefined}
     */
    static resolveIoSchema(methodSchema, io) {
        if (!methodSchema)
            return undefined;
        if (io === 'req')
            return methodSchema.request;
        const response = /** @type {{ response?: Record<string, SchemaRecord> }} */ (methodSchema).response;
        return methodSchema[io] ?? response?.[io];
    }

    /**
     * Request payloads contain framework-provided sections such as headers,
     * paths, query, and body. OPTIONS `request` schemas validate only the
     * sections the app author declares, while each declared section stays
     * strict internally.
     *
     * @param {SchemaRecord} target
     * @param {SchemaRecord} schema
     * @returns {SchemaRecord}
     */
    static declaredRequestSections(target, schema) {
        /** @type {SchemaRecord} */
        const subset = {};
        for (const rawKey of Object.keys(schema)) {
            const key = rawKey.endsWith('?') ? rawKey.slice(0, -1) : rawKey;
            if (key in target)
                subset[key] = target[key];
        }
        return subset;
    }

    /**
     * @param {string} handler
     * @param {string} method - The HTTP method (GET, POST, etc.)
     * @param {string} body
     * @returns {Promise<number | null>}
     */
    static async matchStatusCode(handler, method, body) {
        const relativeRoute = Validate.routeFromHandlerPath(handler);
        const schema = Router.routeConfigs[relativeRoute];
        if (!schema || !method)
            return null;
        const methodSchema = /** @type {MethodSchema | undefined} */ (schema[method]);
        if (!methodSchema)
            return null;
        const statusCodes = Object.keys(methodSchema)
            .filter(k => /^\d{3}$/.test(k))
            .map(Number)
            .sort((a, b) => a - b);
        if (statusCodes.length === 0)
            return null;
        let parsedBody;
        try {
            const candidateBody = JSON.parse(body);
            if (typeof candidateBody !== 'object' || candidateBody === null || Array.isArray(candidateBody))
                return null;
            parsedBody = candidateBody;
        }
        catch {
            return null;
        }
        for (const code of statusCodes) {
            try {
                const statusSchema = methodSchema[String(code)];
                if (!statusSchema)
                    continue;
                await Validate.validateWithChex({ ...parsedBody }, statusSchema, relativeRoute, relativeRoute, method, String(code));
                return code;
            }
            catch {
                continue;
            }
        }
        return null;
    }

    /**
     * @param {string} handler - Absolute handler file path
     * @param {string} method - The HTTP method (GET, POST, etc.)
     * @param {string} io
     * @param {unknown} payload
     */
    static async validateData(handler, method, io, payload) {
        const relativeRoute = Validate.routeFromHandlerPath(handler);
        const schema = Router.routeConfigs[relativeRoute];
        if (!schema || !method) {
            throw new Error(`No validation schema found for route '${relativeRoute}'`);
        }
        /** @param {SchemaRecord} target */
        const applyValidation = async (target) => {
            const methodSchema = /** @type {MethodSchema | undefined} */ (schema[method]);
            const ioSchema = Validate.resolveIoSchema(methodSchema, io);
            if (ioSchema) {
                const validationTarget = io === 'req' && methodSchema?.request
                    ? Validate.declaredRequestSections(target, ioSchema)
                    : target;
                await Validate.validateWithChex(validationTarget, ioSchema, handler, relativeRoute, method, io);
            } else if (io === 'req' && (target.body !== undefined || target.query !== undefined)) {
                throw new Error(
                    `Request data present but no 'request' schema defined for '${method} ${relativeRoute}'`
                );
            }
        };
        if (typeof payload === "string") {
            let parsed;
            try {
                parsed = JSON.parse(payload);
            }
            catch {
                parsed = payload;
            }
            if (typeof parsed === "object" && parsed !== null) {
                await applyValidation(parsed);
            }
        }
        else {
            await applyValidation(/** @type {SchemaRecord} */ (payload));
        }
    }
}
