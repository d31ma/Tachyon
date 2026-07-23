// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { rm } from 'fs/promises';
import os from 'os';
import path from 'path';
import { fileURLToPath } from 'url';

const TEMPLATE_PATH = fileURLToPath(new URL('../../src/compiler/render-template.js', import.meta.url));
/** @type {string[]} */
const tempFiles = [];

afterEach(async () => {
    await Promise.all(tempFiles.splice(0).map((file) => rm(file, { force: true })));
});

/** @param {string} script */
async function buildFactory(script) {
    const source = await Bun.file(TEMPLATE_PATH).text();
    const factorySource = `
const helpers = __tc_helpers__.createTacHelpers({});
${script}
return async function () { return ''; };
`;
    const modified = source
        .replace('// module_imports', '')
        .replace('"__TY_FACTORY_SOURCE__"', () => JSON.stringify(factorySource));
    const tempFile = path.join(os.tmpdir(), `tachyon-native-lifecycle-${Bun.randomUUIDv7()}.js`);
    tempFiles.push(tempFile);
    await Bun.write(tempFile, modified);
    return (await import(tempFile)).default;
}

test('onMount runs after the native bridge is ready and public host.on can subscribe', async () => {
    const previousBridge = (/** @type {any} */ (globalThis)).__tcNativeBridge__;
    const previousResults = (/** @type {any} */ (globalThis)).__tc_test__;
    const results = { mounted: false, events: /** @type {unknown[]} */ ([]) };
    /** @type {Set<(message: unknown) => void>} */
    const listeners = new Set();
    (/** @type {any} */ (globalThis)).__tc_test__ = results;
    (/** @type {any} */ (globalThis)).__tcNativeBridge__ = {
        supports: () => true,
        invoke: async () => ({}),
        onMessage(listener) {
            listeners.add(listener);
            return () => listeners.delete(listener);
        },
    };
    try {
        const factory = await buildFactory(`
__tc_helpers__.onMount(() => {
    globalThis.__tc_test__.mounted = true;
    helpers.__native.host.on('shortcut.activated', (payload) => globalThis.__tc_test__.events.push(payload));
});
`);
        await factory();
        await Promise.resolve();
        for (const listener of listeners)
            listener({ type: 'tac:host-event', event: 'shortcut.activated', payload: { id: 'example.toggle' } });

        expect(results.mounted).toBe(true);
        expect(results.events).toEqual([{ id: 'example.toggle' }]);
    } finally {
        if (previousBridge === undefined) Reflect.deleteProperty(globalThis, '__tcNativeBridge__');
        else (/** @type {any} */ (globalThis)).__tcNativeBridge__ = previousBridge;
        if (previousResults === undefined) Reflect.deleteProperty(globalThis, '__tc_test__');
        else (/** @type {any} */ (globalThis)).__tc_test__ = previousResults;
    }
});
