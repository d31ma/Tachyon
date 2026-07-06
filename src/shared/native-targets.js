// @ts-check
/**
 * @typedef {'web' | 'macos' | 'windows' | 'linux' | 'android' | 'ios'} BundleTarget
 * @typedef {'macos' | 'windows' | 'linux' | 'android' | 'ios'} NativeTarget
 * @typedef {'browser' | 'desktop' | 'mobile'} BundleEnvironment
 * @typedef {'macos' | 'windows' | 'linux' | 'android' | 'ios' | 'unknown'} BundleOS
 * @typedef {{
 *   target: BundleTarget,
 *   platform: BundleTarget,
 *   environment: BundleEnvironment,
 *   os: BundleOS,
 *   native: boolean,
 *   browser: boolean,
 *   web: boolean,
 *   desktop: boolean,
 *   mobile: boolean
 * }} BundleTargetContext
 */

/** Native bundle targets supported by Tac. */
export const NATIVE_TARGETS = /** @type {const} */ (['macos', 'windows', 'linux', 'ios', 'android']);
export const BUNDLE_TARGETS = /** @type {const} */ (['web', 'macos', 'windows', 'linux', 'android', 'ios']);
export const DESKTOP_TARGETS = /** @type {const} */ (['linux', 'windows', 'macos']);
export const MOBILE_TARGETS = /** @type {const} */ (['ios', 'android']);

/** @type {Set<string>} */
export const NATIVE_TARGET_SET = new Set(NATIVE_TARGETS);
/** @type {Set<string>} */
export const BUNDLE_TARGET_SET = new Set(BUNDLE_TARGETS);
/** @type {Set<string>} */
export const DESKTOP_TARGET_SET = new Set(DESKTOP_TARGETS);
/** @type {Set<string>} */
export const MOBILE_TARGET_SET = new Set(MOBILE_TARGETS);

const BUNDLE_TARGET_ALIASES = new Map([
    ['web', 'web'],
    ['browser', 'web'],
    ['mac', 'macos'],
    ['macos', 'macos'],
    ['darwin', 'macos'],
    ['osx', 'macos'],
    ['windows', 'windows'],
    ['win', 'windows'],
    ['win32', 'windows'],
    ['linux', 'linux'],
    ['android', 'android'],
    ['ios', 'ios'],
]);

/**
 * @param {string} target
 * @returns {boolean}
 */
export function isNativeTarget(target) {
    return NATIVE_TARGET_SET.has(target);
}

/**
 * @param {string} target
 * @returns {target is typeof DESKTOP_TARGETS[number]}
 */
export function isDesktopTarget(target) {
    return DESKTOP_TARGET_SET.has(target);
}

/**
 * @param {string} target
 * @returns {target is typeof MOBILE_TARGETS[number]}
 */
export function isMobileTarget(target) {
    return MOBILE_TARGET_SET.has(target);
}

/**
 * @param {string} target
 * @returns {BundleEnvironment}
 */
export function environmentForTarget(target) {
    if (isDesktopTarget(target)) return 'desktop';
    if (isMobileTarget(target)) return 'mobile';
    return 'browser';
}

/**
 * @param {string} target
 * @returns {BundleTarget}
 */
export function platformForTarget(target) {
    return BUNDLE_TARGET_SET.has(target) ? /** @type {BundleTarget} */ (target) : 'web';
}

/**
 * @param {string} target
 * @returns {BundleOS}
 */
export function osForTarget(target) {
    return isNativeTarget(target) ? /** @type {BundleOS} */ (target) : 'unknown';
}

/**
 * @param {BundleTarget} target
 * @returns {BundleTargetContext}
 */
export function targetContext(target) {
    const platform = platformForTarget(target);
    const environment = environmentForTarget(target);
    return {
        target,
        platform,
        environment,
        os: osForTarget(target),
        native: isNativeTarget(target),
        browser: environment === 'browser',
        web: environment === 'browser',
        desktop: environment === 'desktop',
        mobile: environment === 'mobile',
    };
}

/**
 * @param {string[]} argv
 * @returns {string | null}
 */
export function readTargetArg(argv) {
    for (let index = 0; index < argv.length; index += 1) {
        const arg = argv[index];
        if (arg === '--target' || arg === '--targets') {
            // Consume every following non-flag arg so shell-split lists work
            // too: `--target ios, android` and `--target ios android` both
            // read as "ios,android".
            const parts = [];
            for (let next = index + 1; next < argv.length && !argv[next].startsWith('-'); next += 1)
                parts.push(argv[next]);
            return parts.join(',');
        }
        if (arg.startsWith('--target=')) {
            return arg.slice('--target='.length);
        }
        if (arg.startsWith('--targets=')) {
            return arg.slice('--targets='.length);
        }
    }
    return null;
}

/** @param {string} part */
export function normalizeBundleTarget(part) {
    const target = BUNDLE_TARGET_ALIASES.get(part.trim().toLowerCase());
    if (!target) {
        throw new Error(`Unsupported bundle target '${part}'. Use one of: web, macos, windows, linux, android, ios, all.`);
    }
    return target;
}

/** @param {string | null | undefined} raw */
export function resolveBundleTargets(raw) {
    const value = String(raw || 'web').trim();
    if (!value)
        return /** @type {Array<typeof BUNDLE_TARGETS[number]>} */ (['web']);
    const parts = value.split(',').map((part) => part.trim()).filter(Boolean);
    const selected = new Set();
    for (const part of parts) {
        if (part.toLowerCase() === 'all') {
            for (const target of BUNDLE_TARGETS)
                selected.add(target);
            continue;
        }
        selected.add(normalizeBundleTarget(part));
    }
    return /** @type {Array<typeof BUNDLE_TARGETS[number]>} */ ([...selected]);
}

/**
 * @param {string | null | undefined} raw
 * @param {{ allowAll?: boolean }} [options]
 */
export function resolveSingleBundleTarget(raw, options = {}) {
    const targets = resolveBundleTargets(raw);
    if (targets.length !== 1 || (!options.allowAll && String(raw || '').trim().toLowerCase() === 'all')) {
        throw new Error('Expected exactly one target. Use one of: web, macos, windows, linux, android, ios.');
    }
    return targets[0];
}
