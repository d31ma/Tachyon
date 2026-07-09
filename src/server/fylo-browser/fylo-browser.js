// @ts-check
//
// Tachyon Fylo browser route registration — delegates to FyloGateway, which
// drives the FYLO GitHub release binary and adds Tachyon-specific endpoints
// (SSE streaming, deleted-doc management, rebuild, encryption redaction,
// browser UI shell).
//
// Kept as a thin facade so existing callers (Yon.createServerRoutes) and the
// loadCollectionSchema test export don't need to change their import paths.
import path from 'path';
import FyloGateway from './fylo-gateway.js';

/** @typedef {(handler: (request?: Request, server?: import('bun').Server<any>) => Promise<Response> | Response, options?: { route?: string }) => (request?: Request, server?: import('bun').Server<any>) => Promise<Response>} RouteWrapper */

export default class FyloBrowser {
    /** @param {RouteWrapper} [wrapRoute] */
    static registerRoutes(wrapRoute) {
        FyloGateway.registerRoutes(wrapRoute);
    }
}

/**
 * Loads a collection's JSON schema from FYLO_SCHEMA.
 * Exported for integration testing.
 * @param {string} collection
 * @returns {Promise<Record<string, unknown> | null>}
 */
export async function loadCollectionSchema(collection) {
    const schemaDir = process.env.FYLO_SCHEMA || process.env.FYLO_SCHEMA_DIR || process.env.YON_SCHEMA_DIR;
    if (!schemaDir) return null;
    try {
        const manifest = /** @type {{ current?: unknown }} */ (
            await Bun.file(path.join(schemaDir, collection, 'manifest.json')).json()
        );
        const current = typeof manifest.current === 'string' ? manifest.current : '';
        if (current) {
            return /** @type {Record<string, unknown>} */ (
                await Bun.file(path.join(schemaDir, collection, 'history', `${current}.schema.json`)).json()
            );
        }
    } catch { /* fall through to flat path */ }
    try {
        return /** @type {Record<string, unknown>} */ (
            await Bun.file(path.join(schemaDir, `${collection}.json`)).json()
        );
    } catch {
        return null;
    }
}
