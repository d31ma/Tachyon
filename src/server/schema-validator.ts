import Router from "./route-handler.js"

export default class Validate {

    /**
     * Strips modifier characters (`?`, `$`, `^`) from a schema key and wraps
     * keys containing special characters in quotes for safe property access.
     */
    static sanitizePropertyName(key: string): string {
        const cleanKey = key.replace('?', '').replace('$', '').replace('^', '')
        return /^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(cleanKey) ? cleanKey : `"${cleanKey}"`
    }

    /**
     * Recursively validates a data object against a schema definition.
     * Mutates `data` in place when applying nullable default values.
     */
    static validateObject(
        data:         Record<string, unknown>,
        subSchema:    Record<string, unknown>,
        route:        string,
        parentRoute:  string,
        path?:        string,
    ): Record<string, unknown> {

        for (const dataKey in data) {
            if (!(dataKey in subSchema) && !(`^${dataKey}$` in subSchema) && !(`${dataKey}?` in subSchema)) {
                throw new Error(`Property '${dataKey}' does not exist in the '${route}' route schema`)
            }
        }

        for (let schemaKey in subSchema) {

            const schemaValue    = subSchema[schemaKey]
            const sanitizedKey   = Validate.sanitizePropertyName(schemaKey)
            const dataValue      = data[sanitizedKey]
            const valueIsDefined = dataValue !== null && dataValue !== undefined
            const fullPath       = path ? `${path}.${sanitizedKey}` : sanitizedKey
            const isNullable     = schemaKey.endsWith('?')

            schemaKey = isNullable ? schemaKey.replace('?', '') : schemaKey

            const expectedType = typeof schemaValue
            const actualType   = typeof dataValue
            const hasRegex     = schemaKey.startsWith('^') && schemaKey.endsWith('$') && expectedType === "string"

            let regEx: RegExp | undefined

            try {
                if (hasRegex) regEx = new RegExp(schemaValue as string)
            } catch {
                throw new Error(`Invalid RegEx pattern for '${fullPath}' in '${parentRoute}' route`)
            }

            schemaKey = hasRegex ? schemaKey.replace('^', '').replace('$', '') : schemaKey

            const hasDefaultValue = (schemaValue !== "" || !Object.is(schemaValue, -0) || Array.isArray(schemaValue)) && !hasRegex

            if (actualType !== expectedType && !isNullable) {
                throw new Error(
                    `Type mismatch for '${fullPath}' in '${parentRoute}' route: ` +
                    `expected '${expectedType}' but got '${actualType}'`
                )
            }

            if (!valueIsDefined && !isNullable) {
                throw new Error(`Property '${fullPath}' cannot be null or undefined in '${parentRoute}' route`)
            }

            if (valueIsDefined && hasRegex && !regEx!.test(dataValue as string)) {
                throw new Error(`RegEx pattern fails for property '${fullPath}' in '${parentRoute}' route`)
            }

            if (!valueIsDefined && isNullable && hasDefaultValue) {
                data[schemaKey] = schemaValue
            }

            if (valueIsDefined && expectedType === "object" && !Array.isArray(dataValue)) {
                const entries = Object.entries(schemaValue as Record<string, unknown>)
                const isEmpty = entries.some(([key]) => key === "")

                if (!isEmpty) {
                    data[schemaKey] = Validate.validateObject(
                        dataValue as Record<string, unknown>,
                        schemaValue as Record<string, unknown>,
                        route, parentRoute, fullPath,
                    )
                } else {
                    const [, value] = entries[0]
                    for (const [k, v] of Object.entries(dataValue as Record<string, unknown>)) {
                        if (typeof v !== typeof value) {
                            throw new Error(
                                `Type mismatch for '${fullPath}.${k}' in '${parentRoute}' route: ` +
                                `expected '${typeof value}' but got '${typeof v}'`
                            )
                        }
                    }
                }
            }

            if (valueIsDefined && expectedType === "object" && Array.isArray(dataValue) && Array.isArray(schemaValue)) {
                const dataTypes   = Array.from(new Set((dataValue  as unknown[]).map(v => typeof v)))
                const schemaTypes = Array.from(new Set((schemaValue as unknown[]).map(v => typeof v)))

                for (const dataType of dataTypes) {
                    if (!schemaTypes.includes(dataType)) {
                        throw new Error(
                            `Type mismatch for '${fullPath}' in '${parentRoute}' route: ` +
                            `'${dataType}' is not included in [${schemaTypes.join(',')}]`
                        )
                    }
                }
            }
        }

        return data
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
    static matchStatusCode(handler: string, body: string): number | null {
        const parts        = handler.split('/')
        const method       = parts.pop()!
        const absoluteDir  = parts.join('/')
        const relativeRoute = absoluteDir.replace(Router.routesPath, '') || '/'

        const schema = Router.routeConfigs[relativeRoute] as unknown as Record<string, Record<string, Record<string, unknown>>>
        if (!schema) return null

        const methodSchema = schema[method]
        if (!methodSchema) return null

        const statusCodes = Object.keys(methodSchema)
            .filter(k => /^\d{3}$/.test(k))
            .map(Number)
            .sort((a, b) => a - b)

        if (statusCodes.length === 0) return null

        let parsed: Record<string, unknown>
        try {
            const p = JSON.parse(body)
            if (typeof p !== 'object' || p === null || Array.isArray(p)) return null
            parsed = p
        } catch {
            return null
        }

        for (const code of statusCodes) {
            try {
                Validate.validateObject({ ...parsed }, methodSchema[String(code)], relativeRoute, relativeRoute)
                return code
            } catch {}
        }

        return null
    }

    static async validateData(
        route: string,
        io:    string,
        data:  Record<string, unknown> | string,
    ) {
        const parts        = route.split('/')
        const method       = parts.pop()!
        const absoluteDir  = parts.join('/')
        const relativeRoute = absoluteDir.replace(Router.routesPath, '') || '/'

        const schema = Router.routeConfigs[relativeRoute] as unknown as Record<string, unknown>

        if (!schema) {
            throw new Error(`No validation schema found for route '${relativeRoute}'`)
        }

        const applyValidation = (target: Record<string, unknown>) => {
            const methodSchema = (schema as Record<string, Record<string, Record<string, unknown>>>)[method]
            if (methodSchema?.[io]) {
                Validate.validateObject(target, methodSchema[io], route, relativeRoute)
            }
        }

        if (typeof data === "string") {
            let parsed: unknown

            try { parsed = JSON.parse(data) }
            catch { parsed = data }

            if (typeof parsed === "object" && parsed !== null) {
                applyValidation(parsed as Record<string, unknown>)
            }
        } else {
            applyValidation(data)
        }
    }
}
