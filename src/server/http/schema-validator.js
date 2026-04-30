// @ts-check
import { validateData as chexValidateData } from '@d31ma/chex';
import Router from "./route-handler.js";

/** @typedef {Record<string, any>} SchemaRecord */
/** @typedef {Record<string, SchemaRecord>} MethodSchema */

export default class Validate {
    /** @type {Map<string, SchemaRecord>} */
    static chexSchemas = new Map();

    /**
     * @param {string | undefined} filename
     * @returns {string | undefined}
     */
    static methodFromHandlerFilename(filename) {
        return filename?.split('.', 1)[0];
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
        Validate.chexSchemas.set(key, schema);
        await chexValidateData(key, data, { schemaDir: null, cache: Validate.chexSchemas });
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
     * @param {string} body
     * @returns {Promise<number | null>}
     */
    static async matchStatusCode(handler, body) {
        const normalizedHandler = handler.replaceAll('\\', '/');
        const normalizedRoutesPath = Router.routesPath.replaceAll('\\', '/');
        const parts = normalizedHandler.split('/');
        const method = Validate.methodFromHandlerFilename(parts.pop());
        const absoluteDir = parts.join('/');
        const relativeRoute = Router.filesystemPathToRoute(absoluteDir.replace(normalizedRoutesPath, '') || '/');
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
        let parsed;
        try {
            const p = JSON.parse(body);
            if (typeof p !== 'object' || p === null || Array.isArray(p))
                return null;
            parsed = p;
        }
        catch {
            return null;
        }
        for (const code of statusCodes) {
            try {
                const statusSchema = methodSchema[String(code)];
                if (!statusSchema)
                    continue;
                await Validate.validateWithChex({ ...parsed }, statusSchema, relativeRoute, relativeRoute, method, String(code));
                return code;
            }
            catch { }
        }
        return null;
    }
    /**
     * @param {string} route
     * @param {string} io
     * @param {unknown} data
     */
    static async validateData(route, io, data) {
        const normalizedRoute = route.replaceAll('\\', '/');
        const normalizedRoutesPath = Router.routesPath.replaceAll('\\', '/');
        const parts = normalizedRoute.split('/');
        const method = Validate.methodFromHandlerFilename(parts.pop());
        const absoluteDir = parts.join('/');
        const relativeRoute = Router.filesystemPathToRoute(absoluteDir.replace(normalizedRoutesPath, '') || '/');
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
                await Validate.validateWithChex(validationTarget, ioSchema, route, relativeRoute, method, io);
            }
        };
        if (typeof data === "string") {
            let parsed;
            try {
                parsed = JSON.parse(data);
            }
            catch {
                parsed = data;
            }
            if (typeof parsed === "object" && parsed !== null) {
                await applyValidation(parsed);
            }
        }
        else {
            await applyValidation(/** @type {SchemaRecord} */ (data));
        }
    }
}
