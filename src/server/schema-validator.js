// @ts-check
import Router from "./route-handler.js";

/** @typedef {Record<string, any>} SchemaRecord */
/** @typedef {Record<string, SchemaRecord>} MethodSchema */

export default class Validate {
    /**
     * Strips modifier characters (`?`, `$`, `^`) from a schema key and wraps
     * keys containing special characters in quotes for safe property access.
     */
    /** @param {string} key */
    static sanitizePropertyName(key) {
        const cleanKey = key.replace('?', '').replace('$', '').replace('^', '');
        return /^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(cleanKey) ? cleanKey : `"${cleanKey}"`;
    }
    /**
     * Recursively validates a data object against a schema definition.
     * Mutates `data` in place when applying nullable default values.
     */
    /**
     * @param {SchemaRecord} data
     * @param {SchemaRecord} subSchema
     * @param {string} route
     * @param {string} parentRoute
     * @param {string} [path]
     */
    static validateObject(data, subSchema, route, parentRoute, path = '') {
        for (const dataKey in data) {
            if (!(dataKey in subSchema) && !(`^${dataKey}$` in subSchema) && !(`${dataKey}?` in subSchema)) {
                throw new Error(`Property '${dataKey}' does not exist in the '${route}' route schema`);
            }
        }
        for (let schemaKey in subSchema) {
            const schemaValue = subSchema[schemaKey];
            const sanitizedKey = Validate.sanitizePropertyName(schemaKey);
            const dataValue = data[sanitizedKey];
            const valueIsDefined = dataValue !== null && dataValue !== undefined;
            const fullPath = path ? `${path}.${sanitizedKey}` : sanitizedKey;
            const isNullable = schemaKey.endsWith('?');
            schemaKey = isNullable ? schemaKey.replace('?', '') : schemaKey;
            const expectedType = typeof schemaValue;
            const actualType = typeof dataValue;
            const hasRegex = schemaKey.startsWith('^') && schemaKey.endsWith('$') && expectedType === "string";
            /** @type {RegExp | undefined} */
            let regEx;
            try {
                if (hasRegex)
                    regEx = new RegExp(schemaValue);
            }
            catch {
                throw new Error(`Invalid RegEx pattern for '${fullPath}' in '${parentRoute}' route`);
            }
            schemaKey = hasRegex ? schemaKey.replace('^', '').replace('$', '') : schemaKey;
            const hasDefaultValue = (schemaValue !== "" || !Object.is(schemaValue, -0) || Array.isArray(schemaValue)) && !hasRegex;
            if (actualType !== expectedType && !isNullable) {
                throw new Error(`Type mismatch for '${fullPath}' in '${parentRoute}' route: ` +
                    `expected '${expectedType}' but got '${actualType}'`);
            }
            if (!valueIsDefined && !isNullable) {
                throw new Error(`Property '${fullPath}' cannot be null or undefined in '${parentRoute}' route`);
            }
            if (valueIsDefined && hasRegex && regEx && !regEx.test(String(dataValue))) {
                throw new Error(`RegEx pattern fails for property '${fullPath}' in '${parentRoute}' route`);
            }
            if (!valueIsDefined && isNullable && hasDefaultValue) {
                data[schemaKey] = schemaValue;
            }
            if (valueIsDefined && expectedType === "object" && !Array.isArray(dataValue)) {
                const entries = Object.entries(schemaValue);
                const isEmpty = entries.some(([key]) => key === "");
                if (!isEmpty) {
                    data[schemaKey] = Validate.validateObject(dataValue, schemaValue, route, parentRoute, fullPath);
                }
                else {
                    const [, value] = entries[0];
                    for (const [k, v] of Object.entries(dataValue)) {
                        if (typeof v !== typeof value) {
                            throw new Error(`Type mismatch for '${fullPath}.${k}' in '${parentRoute}' route: ` +
                                `expected '${typeof value}' but got '${typeof v}'`);
                        }
                    }
                }
            }
            if (valueIsDefined && expectedType === "object" && Array.isArray(dataValue) && Array.isArray(schemaValue)) {
                const dataTypes = Array.from(new Set(dataValue.map(v => typeof v)));
                const schemaTypes = Array.from(new Set(schemaValue.map(v => typeof v)));
                for (const dataType of dataTypes) {
                    if (!schemaTypes.includes(dataType)) {
                        throw new Error(`Type mismatch for '${fullPath}' in '${parentRoute}' route: ` +
                            `'${dataType}' is not included in [${schemaTypes.join(',')}]`);
                    }
                }
            }
        }
        return data;
    }
    /**
     * Validates `data` against the schema loaded at startup for `route`.
     * Uses {@link Router.routeConfigs} — the single source of truth already
     * populated during `validateRoutes()` — instead of re-reading OPTIONS
     * files from disk.
     */
    /**
     * Tries to match `body` against each numeric status-code schema defined for
     * the route/method in the OPTIONS file. Returns the first matching code, or
     * `null` if no numeric schemas are defined or none match.
     */
    /**
     * @param {string} handler
     * @param {string} body
     * @returns {number | null}
     */
    static matchStatusCode(handler, body) {
        const parts = handler.split('/');
        const method = parts.pop();
        const absoluteDir = parts.join('/');
        const relativeRoute = Router.filesystemPathToRoute(absoluteDir.replace(Router.routesPath, '') || '/');
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
                Validate.validateObject({ ...parsed }, statusSchema, relativeRoute, relativeRoute);
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
        const parts = route.split('/');
        const method = parts.pop();
        const absoluteDir = parts.join('/');
        const relativeRoute = Router.filesystemPathToRoute(absoluteDir.replace(Router.routesPath, '') || '/');
        const schema = Router.routeConfigs[relativeRoute];
        if (!schema || !method) {
            throw new Error(`No validation schema found for route '${relativeRoute}'`);
        }
        /** @param {SchemaRecord} target */
        const applyValidation = (target) => {
            const methodSchema = /** @type {MethodSchema | undefined} */ (schema[method]);
            const ioSchema = methodSchema?.[io];
            if (ioSchema) {
                Validate.validateObject(target, ioSchema, route, relativeRoute);
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
                applyValidation(parsed);
            }
        }
        else {
            applyValidation(/** @type {SchemaRecord} */ (data));
        }
    }
}
