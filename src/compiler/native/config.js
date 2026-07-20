// @ts-check
import path from 'path';
import { readFile } from 'fs/promises';

export const DEVICE_PERMISSION_NAMES = Object.freeze([
    'camera',
    'microphone',
    'location',
    'notifications',
    'screenCapture',
]);

export const RAW_NATIVE_CAPABILITY_NAMES = Object.freeze([
    'fs.readText',
    'fs.writeText',
    'fs.readDir',
    'fs.stat',
    'fs.mkdir',
    'fs.remove',
    'shell.exec',
]);

/**
 * @param {unknown} value
 * @param {string} key
 * @param {readonly string[]} allowed
 */
function validateStringList(value, key, allowed) {
    if (value === undefined) return [];
    if (!Array.isArray(value)) throw new Error(`${key} must be an array`);
    const allowedSet = new Set(allowed);
    const entries = value.map((entry) => {
        if (typeof entry !== 'string' || !allowedSet.has(entry)) {
            throw new Error(`${key} contains unsupported value '${String(entry)}'. Supported: ${allowed.join(', ')}`);
        }
        return entry;
    });
    return [...new Set(entries)].sort();
}

/** @param {unknown} value @param {string} key */
function validateOriginList(value, key) {
    if (value === undefined) return [];
    if (!Array.isArray(value)) throw new Error(`${key} must be an array`);
    return [...new Set(value.map((entry) => {
        if (typeof entry !== 'string')
            throw new Error(`${key} entries must be an exact HTTPS origin`);
        let url;
        try { url = new URL(entry); }
        catch { throw new Error(`${key} entries must be an exact HTTPS origin`); }
        if (url.protocol !== 'https:' || url.username || url.password || url.pathname !== '/' || url.search || url.hash || entry.includes('*'))
            throw new Error(`${key} entries must be an exact HTTPS origin`);
        return url.origin;
    }))].sort();
}

/** @param {unknown} value */
function validatePermissionOrigins(value) {
    if (value === undefined) return {};
    if (!value || typeof value !== 'object' || Array.isArray(value))
        throw new Error('tachyon.permissionOrigins must be an object');
    const source = /** @type {Record<string, unknown>} */ (value);
    for (const key of Object.keys(source)) {
        if (key !== 'camera' && key !== 'microphone')
            throw new Error(`tachyon.permissionOrigins contains unsupported permission '${key}'`);
    }
    return Object.fromEntries(['camera', 'microphone']
        .filter((key) => source[key] !== undefined)
        .map((key) => [key, validateOriginList(source[key], `tachyon.permissionOrigins.${key}`)]));
}

/**
 * Reads the native security declarations from package.json. Missing package
 * metadata is allowed; malformed or unsupported declarations fail closed.
 * @param {string} [root]
 */
export async function resolveNativeAppConfig(root = process.cwd()) {
    let source;
    try {
        source = await readFile(path.join(root, 'package.json'), 'utf8');
    }
    catch (error) {
        if (/** @type {NodeJS.ErrnoException} */ (error).code === 'ENOENT') {
            return { devicePermissions: [], nativeCapabilities: [], permissionOrigins: {}, managedContentOrigins: [] };
        }
        throw error;
    }
    let pkg;
    try {
        pkg = JSON.parse(source);
    }
    catch (error) {
        throw new Error(`Unable to parse package.json: ${error instanceof Error ? error.message : String(error)}`);
    }
    const config = pkg.tachyon ?? {};
    const legacyConfig = pkg.tac ?? {};
    if (config === null || typeof config !== 'object' || Array.isArray(config)) {
        throw new Error('tachyon must be an object');
    }
    if (legacyConfig === null || typeof legacyConfig !== 'object' || Array.isArray(legacyConfig)) {
        throw new Error('tac must be an object');
    }
    return {
        devicePermissions: validateStringList(config.devicePermissions ?? legacyConfig.devicePermissions, 'tachyon.devicePermissions', DEVICE_PERMISSION_NAMES),
        nativeCapabilities: validateStringList(config.nativeCapabilities ?? legacyConfig.nativeCapabilities, 'tachyon.nativeCapabilities', RAW_NATIVE_CAPABILITY_NAMES),
        permissionOrigins: validatePermissionOrigins(config.permissionOrigins ?? legacyConfig.permissionOrigins),
        managedContentOrigins: validateOriginList(config.managedContentOrigins ?? legacyConfig.managedContentOrigins, 'tachyon.managedContentOrigins'),
    };
}
