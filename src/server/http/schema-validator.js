// @ts-check
import { validateData as chexValidateData } from '@d31ma/chex';
import { mkdtempSync } from 'fs';
import { tmpdir } from 'os';
import path from 'path';
import Router from "./route-handler.js";

/** @typedef {Record<string, any>} SchemaRecord */
/** @typedef {Record<string, SchemaRecord>} MethodSchema */

export default class Validate {
    /** @type {string | null} */
    static chexSchemaDir = null;

    /** @type {Map<string, SchemaRecord>} */
    static chexSchemas = new Map();

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
     * @param {string} route
     * @param {string | undefined} method
     * @param {string} io
     * @returns {string}
     */
    static schemaCacheKey(route, method, io) {
        const digest = new Bun.CryptoHasher('sha256')
            .update(`${route}:${method ?? ''}:${io}`)
            .digest('hex')
            .slice(0, 16);
        return `tachyon_${digest}`;
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
        const key = Validate.schemaCacheKey(parentRoute, method, io);
        // CHEX treats schema references ending in .schema.json as paths and
        // looks up its cache under a 'path:'-prefixed key.  Seed the cache
        // with the exact key CHEX will request so it skips disk I/O.
        const chexKey = `${key}.schema.json`;
        Validate.chexSchemas.set(`path:${chexKey}`, schema);
        await chexValidateData(chexKey, data, { schemaDir: null, cache: Validate.chexSchemas });
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
