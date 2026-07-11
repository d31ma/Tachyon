// @ts-check
import { expect, test } from 'bun:test';
import { checkNativePreviewRequirements, formatNativePreviewCheckFailure } from '../../src/shared/native-preview-checks.js';
import { osForTarget, platformForTarget, resolveBundleTargets, resolveSingleBundleTarget, targetContext } from '../../src/shared/native-targets.js';

test('bundle target resolver normalizes aliases and all targets', () => {
    expect(resolveSingleBundleTarget('browser')).toBe('web');
    expect(resolveSingleBundleTarget('MacOS')).toBe('macos');
    expect(resolveBundleTargets('web,win,ios')).toEqual(['web', 'windows', 'ios']);
    expect(resolveBundleTargets('all')).toEqual(['web', 'macos', 'windows', 'linux', 'android', 'ios']);
});

test('bundle target context uses the rooted terminology: platform is the form factor, environment/os the host', () => {
    expect(platformForTarget('web')).toBe('web');
    expect(osForTarget('web')).toBe('web');
    for (const target of ['linux', 'windows', 'macos']) {
        expect(platformForTarget(target)).toBe('desktop');
        expect(osForTarget(target)).toBe(target);
        expect(targetContext(/** @type {any} */ (target))).toMatchObject({
            target,
            platform: 'desktop',
            environment: target,
            os: target,
            native: true,
            desktop: true,
            mobile: false,
            web: false,
        });
    }
    for (const target of ['ios', 'android']) {
        expect(platformForTarget(target)).toBe('mobile');
        expect(osForTarget(target)).toBe(target);
        expect(targetContext(/** @type {any} */ (target))).toMatchObject({
            target,
            platform: 'mobile',
            environment: target,
            os: target,
            native: true,
            desktop: false,
            mobile: true,
            web: false,
        });
    }
    expect(targetContext('web')).toMatchObject({
        target: 'web',
        platform: 'web',
        environment: 'web',
        os: 'web',
        native: false,
        web: true,
    });
});

test('native preview checks skip web target', async () => {
    const result = await checkNativePreviewRequirements('web', {
        platform: 'linux',
        commandAvailable: async () => false,
        pathExists: async () => false,
    });
    expect(result.ok).toBe(true);
    expect(result.missing).toEqual([]);
});

test('native preview checks report platform and toolchain gaps', async () => {
    const result = await checkNativePreviewRequirements('ios', {
        platform: 'linux',
        commandAvailable: async () => false,
    });
    expect(result.ok).toBe(false);
    expect(result.missing).toContain('macOS host');
    expect(result.missing).toContain('xcodebuild');
    expect(formatNativePreviewCheckFailure('ios', result)).toContain("Cannot preview native target 'ios'");
});

test('android preview checks accept configured SDK and JDK', async () => {
    const result = await checkNativePreviewRequirements('android', {
        platform: 'darwin',
        env: { ANDROID_HOME: '/opt/android-sdk' },
        commandAvailable: async (command) => command === 'java',
        pathExists: async (filePath) => filePath === '/opt/android-sdk',
    });
    expect(result.ok).toBe(true);
});
