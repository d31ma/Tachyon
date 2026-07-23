// @ts-check
/**
 * The native host capability catalog is the boundary between Tac companions
 * and platform generators. A capability only routes to a host when that host
 * has a concrete implementation; otherwise the browser fallback stays active.
 */

const BASE_CAPABILITIES = Object.freeze([
    'app.info',
    'capabilities.state',
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
    'window.state',
    'window.alwaysOnTop',
    'window.opacity',
]);

const MANAGED_CONTENT_CAPABILITIES = Object.freeze([
    'contentSurface.open',
    'contentSurface.navigate',
    'contentSurface.state',
    'contentSurface.goBack',
    'contentSurface.goForward',
    'contentSurface.reload',
    'contentSurface.close',
]);

const DESKTOP_SHORTCUT_CAPABILITIES = Object.freeze([
    'shortcuts.register',
    'shortcuts.unregister',
    'shortcuts.unregisterAll',
    'shortcuts.list',
]);

const DESKTOP_RECOVERABLE_WINDOW_CAPABILITIES = Object.freeze([
    'window.clickThrough',
    'window.captureProtection',
]);

const SCREEN_CAPTURE_CAPABILITIES = Object.freeze([
    'screenCapture.state',
    'screenCapture.listWindows',
    'screenCapture.captureWindow',
]);

const FILESYSTEM_CAPABILITIES = Object.freeze(['fs.readText', 'fs.writeText', 'fs.readDir', 'fs.stat', 'fs.mkdir', 'fs.remove']);

const SECURE_STORAGE_CAPABILITIES = Object.freeze([
    'secrets.get',
    'secrets.set',
    'secrets.delete',
    'auth.verifyUser',
]);

/** @param {string} target @param {string[]} [requestedRawCapabilities] @param {{ devicePermissions?: string[], extensionCapabilities?: string[], managedContentOrigins?: string[] }} [options] */
export function nativeHostCapabilities(target, requestedRawCapabilities = [], options = {}) {
    const requested = new Set(requestedRawCapabilities);
    const devicePermissions = new Set(Array.isArray(options.devicePermissions) ? options.devicePermissions : []);
    const extensionCapabilities = Array.isArray(options.extensionCapabilities) ? options.extensionCapabilities : [];
    const managedContent = Array.isArray(options.managedContentOrigins) && options.managedContentOrigins.length > 0
        ? MANAGED_CONTENT_CAPABILITIES
        : [];
    const raw = [
        ...(target === 'macos'
            ? FILESYSTEM_CAPABILITIES.filter((capability) => requested.has(capability))
            : []),
        ...target === 'macos' && requested.has('shell.exec') ? ['shell.exec'] : [],
    ];
    if (target === 'android') return Object.freeze([...MOBILE_CAPABILITIES, 'ui.statusBarStyle', ...SECURE_STORAGE_CAPABILITIES, ...raw]);
    if (target === 'ios') return Object.freeze([...MOBILE_CAPABILITIES, ...SECURE_STORAGE_CAPABILITIES, ...raw]);
    if (target === 'macos') return Object.freeze([
        ...DESKTOP_CAPABILITIES,
        ...managedContent,
        ...DESKTOP_SHORTCUT_CAPABILITIES,
        ...DESKTOP_RECOVERABLE_WINDOW_CAPABILITIES,
        ...(devicePermissions.has('screenCapture') ? SCREEN_CAPTURE_CAPABILITIES : []),
        'fs.paths',
        ...raw,
        ...extensionCapabilities,
    ]);
    if (target === 'windows') return Object.freeze([
        ...DESKTOP_CAPABILITIES,
        ...managedContent,
        ...DESKTOP_SHORTCUT_CAPABILITIES,
        ...DESKTOP_RECOVERABLE_WINDOW_CAPABILITIES,
        ...(devicePermissions.has('screenCapture') ? SCREEN_CAPTURE_CAPABILITIES : []),
        ...raw,
        ...extensionCapabilities,
    ]);
    if (target === 'linux') return Object.freeze([
        ...DESKTOP_CAPABILITIES,
        ...managedContent,
        ...DESKTOP_SHORTCUT_CAPABILITIES,
        'window.clickThrough',
        ...(devicePermissions.has('screenCapture') ? SCREEN_CAPTURE_CAPABILITIES : []),
        ...raw,
        ...extensionCapabilities,
    ]);
    return Object.freeze([]);
}

/** @param {string} target @param {string[]} [requestedRawCapabilities] @param {{ devicePermissions?: string[], managedContentOrigins?: string[] }} [options] */
export function nativeRawHostCapabilities(target, requestedRawCapabilities = [], options = {}) {
    return nativeHostCapabilities(target, requestedRawCapabilities, options).filter((capability) => /^(?:fs\.(?:readText|writeText|readDir|stat|mkdir|remove)|shell\.|process\.)/.test(capability));
}

export const TAC_NATIVE_BRIDGE_ABI_VERSION = 1;
