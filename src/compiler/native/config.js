// @ts-check
import path from 'path';
import { readFile } from 'fs/promises';

export const DEVICE_PERMISSION_NAMES = Object.freeze([
    'camera',
    'microphone',
    'location',
    'notifications',
]);

export const RAW_NATIVE_CAPABILITY_NAMES = Object.freeze([
    'fs.readText',
    'fs.writeText',
    'fs.readDir',
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
            return { devicePermissions: [], nativeCapabilities: [] };
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
    };
}
