// @ts-check
import { afterEach, describe, expect, test } from 'bun:test';
import Compiler from '../../src/compiler/index.js';

const COMPONENT = 'island-counter';

afterEach(() => Compiler.compMapping.delete(COMPONENT));

/** Compile a page containing one known component and return its generated module. */
async function compile(invocation, publicPath = '/pages/islands/tac.js') {
    Compiler.compMapping.set(COMPONENT, 'counter/tac.js');
    const nodes = await Compiler.parseHTML(`<main>${invocation}</main>`, publicPath);
    return Compiler.createJSData({ script: '', scriptLang: 'js' }, nodes, publicPath);
}

describe('component island compilation', () => {
    for (const policy of ['load', 'idle', 'visible', 'interaction']) {
        test(`hydrate="${policy}" emits a populated SSR island boundary`, async () => {
            const code = await compile(`<${COMPONENT} hydrate="${policy}" label="Count" />`);
            const generated = code.replaceAll('\\"', '"');

            expect(generated).toContain('data-tac-island');
            expect(generated).toContain(`data-tac-hydrate="${policy}"`);
            expect(generated).toContain(`data-tac-scope="${COMPONENT}"`);
            expect(generated).toContain('data-tac-module=');
            expect(generated).toContain('data-tac-props=');
            // Islands are server-rendered; unlike `lazy`, their host is not empty.
            expect(generated).toContain('elements += await render(');
            expect(generated).not.toContain('data-lazy-component');
            // The policy is compiler metadata, never a child component prop.
            expect(code.replaceAll('\\"', '"')).not.toMatch(/"hydrate"\s*:/);
        });
    }

    test('hydrate="never" emits SSR content without client activation metadata', async () => {
        const code = await compile(`<${COMPONENT} hydrate="never" label="Static" />`);

        expect(code).toContain('data-tac-island-static');
        expect(code).toContain('elements += await render(');
        expect(code).not.toContain('data-tac-module=');
        expect(code).not.toContain('data-tac-props=');
        expect(code.replaceAll('\\"', '"')).not.toMatch(/"hydrate"\s*:/);
    });

    test('island ids and generated output are deterministic for the same module', async () => {
        const invocation = `<${COMPONENT} hydrate="visible" :count="1" />`;
        expect(await compile(invocation)).toBe(await compile(invocation));
    });

    test('rejects an unsupported hydration policy with the component and allowed values', async () => {
        await expect(compile(`<${COMPONENT} hydrate="hover" />`)).rejects.toThrow(
            /island-counter[\s\S]*hydrate[\s\S]*load[\s\S]*idle[\s\S]*visible[\s\S]*interaction[\s\S]*never/i,
        );
    });

    test('rejects an empty hydration policy instead of dropping the component import', async () => {
        await expect(compile(`<${COMPONENT} hydrate="" />`))
            .rejects.toThrow(/hydrate="" is unsupported.*load, idle, visible, interaction, never/);
    });

    test('rejects a dynamic hydration policy because scheduling is build-time metadata', async () => {
        await expect(compile(`<${COMPONENT} :hydrate="policy" />`)).rejects.toThrow(
            /hydrate[\s\S]*(literal|build.?time|static)/i,
        );
    });

    test('rejects hydrate and lazy on the same component', async () => {
        await expect(compile(`<${COMPONENT} hydrate="visible" lazy />`)).rejects.toThrow(
            /island-counter[\s\S]*(hydrate[\s\S]*lazy|lazy[\s\S]*hydrate)/i,
        );
    });
});
