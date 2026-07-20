// @ts-check
import { describe, expect, test } from 'bun:test';
import {
    readRenderModeArg,
    resolveRenderMode,
    resolveRenderModes,
} from '../../src/shared/render-mode.js';

describe('native-first rendering policy', () => {
    test('web uses the DOM while every non-web target defaults to native UI', () => {
        expect(resolveRenderMode('web')).toBe('web');
        for (const target of ['macos', 'windows', 'linux', 'ios', 'android'])
            expect(resolveRenderMode(target)).toBe('native');
    });

    test('the removed render-mode flag fails with migration guidance', () => {
        expect(() => readRenderModeArg(['ty', 'bundle', '--render-mode', 'webview'])).toThrow(/has been removed.*native-first/i);
        expect(() => readRenderModeArg(['ty', 'bundle', '--render-mode=hybrid'])).toThrow(/has been removed.*native-first/i);
        expect(() => resolveRenderMode('ios', 'webview')).toThrow(/explicit render modes have been removed/i);
    });

    test('mixed bundles keep web on the DOM and make every non-web target native-first', () => {
        expect(resolveRenderModes(['web', 'macos', 'android'])).toEqual({
            web: 'web',
            macos: 'native',
            android: 'native',
        });
    });

    test('the legacy environment override is rejected too', () => {
        expect(() => resolveRenderMode('linux', 'browser')).toThrow(/TAC_RENDER_MODE.*removed/i);
    });
});
