// @ts-check
/**
 * The native host capability catalog is the boundary between Tac companions
 * and platform generators. A capability only routes to a host when that host
 * has a concrete implementation; otherwise the browser fallback stays active.
 */

const BASE_CAPABILITIES = Object.freeze([
    'app.info',
    'clipboard.readText',
    'clipboard.writeText',
    'openUrl',
]);

const MOBILE_CAPABILITIES = Object.freeze([
    ...BASE_CAPABILITIES,
    'share.text',
    'haptics.impact',
    'fs.paths',
]);

const DESKTOP_CAPABILITIES = Object.freeze([
    ...BASE_CAPABILITIES,
]);

const FILESYSTEM_CAPABILITIES = Object.freeze(['fs.readText', 'fs.writeText', 'fs.readDir', 'fs.stat', 'fs.mkdir', 'fs.remove']);

const SECURE_STORAGE_CAPABILITIES = Object.freeze([
    'secrets.get',
    'secrets.set',
    'secrets.delete',
    'auth.verifyUser',
]);

/** @param {string} target @param {string[]} [requestedRawCapabilities] */
export function nativeHostCapabilities(target, requestedRawCapabilities = []) {
    const requested = new Set(requestedRawCapabilities);
    const raw = [
        ...(target === 'android' || target === 'ios' || target === 'macos' || target === 'windows' || target === 'linux'
            ? FILESYSTEM_CAPABILITIES.filter((capability) => requested.has(capability))
            : []),
        ...(target === 'macos' || target === 'windows' || target === 'linux') && requested.has('shell.exec') ? ['shell.exec'] : [],
    ];
    if (target === 'android') return Object.freeze([...MOBILE_CAPABILITIES, 'ui.statusBarStyle', ...SECURE_STORAGE_CAPABILITIES, ...raw]);
    if (target === 'ios') return Object.freeze([...MOBILE_CAPABILITIES, ...SECURE_STORAGE_CAPABILITIES, ...raw]);
    if (target === 'macos') return Object.freeze([...DESKTOP_CAPABILITIES, 'fs.paths', ...SECURE_STORAGE_CAPABILITIES, ...raw]);
    if (target === 'windows' || target === 'linux') return Object.freeze([...DESKTOP_CAPABILITIES, ...raw]);
    return Object.freeze([]);
}

/** @param {string} target @param {string[]} [requestedRawCapabilities] */
export function nativeRawHostCapabilities(target, requestedRawCapabilities = []) {
    return nativeHostCapabilities(target, requestedRawCapabilities).filter((capability) => /^(?:fs\.(?:readText|writeText|readDir|stat|mkdir|remove)|shell\.|process\.)/.test(capability));
}

export const TAC_NATIVE_BRIDGE_ABI_VERSION = 1;
