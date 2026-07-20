// @ts-check
import { isNativeTarget } from './native-targets.js';

export const NATIVE_RENDER_MODES = /** @type {const} */ (['native']);

/**
 * @param {string[]} argv
 * @returns {string | null}
 */
export function readRenderModeArg(argv) {
    if (argv.some((argument) => argument === '--render-mode' || argument.startsWith('--render-mode=')))
        throw new Error('--render-mode has been removed. Non-web targets are always native-first and automatically use local WebView boundaries for unmapped HTML and Web Components.');
    return null;
}

/**
 * @param {string} target
 * @param {string | null | undefined} raw
 * @returns {'web' | 'native'}
 */
export function resolveRenderMode(target, raw = null) {
    if (raw != null && String(raw).trim())
        throw new Error('TAC_RENDER_MODE and explicit render modes have been removed. Non-web targets are always native-first.');
    if (!isNativeTarget(target))
        return 'web';
    return 'native';
}

/**
 * @param {string[]} targets
 * @param {string | null | undefined} raw
 * @returns {Record<string, 'web' | 'native'>}
 */
export function resolveRenderModes(targets, raw = null) {
    return Object.fromEntries(targets.map((target) => [target, resolveRenderMode(target, raw)]));
}
