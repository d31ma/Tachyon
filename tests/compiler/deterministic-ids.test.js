// @ts-check
import { afterAll, beforeAll, describe, expect, test } from 'bun:test';
import Compiler from '../../src/compiler/index.js';

const MAX_COMPONENTS = 2000;
const registered = [];
beforeAll(() => {
    // Register gen-c0..gen-cN so bigPage's <gen-cI/> tags resolve as components
    // (and therefore mint component-host ids we can check for collisions).
    for (let i = 0; i < MAX_COMPONENTS; i++) {
        registered.push(`gen-c${i}`);
        Compiler.compMapping.set(`gen-c${i}`, `gen/c${i}/tac.js`);
    }
});
afterAll(() => {
    for (const name of registered)
        Compiler.compMapping.delete(name);
});

/** Compile a page template to its render-source string. */
async function compile(html, name) {
    const nodes = await Compiler.parseHTML(html, name);
    return Compiler.createJSData({ script: '', scriptLang: 'js' }, nodes, name);
}

/** Every generated id embedded in a compiled module (event + generated + host). */
function idsIn(code) {
    return [
        ...[...code.matchAll(/tc_invokeEvent\('([a-z0-9]+)'/g)].map((m) => m[1]),
        ...[...code.matchAll(/tc_generateId\('([a-z0-9]+)'/g)].map((m) => m[1]),
        ...[...code.matchAll(/__tc_host_([a-z0-9]+)\b/g)].map((m) => m[1]),
    ];
}

/** A page that mints many ids: K component refs + K event handlers + K interpolations. */
function bigPage(k) {
    const parts = [];
    for (let i = 0; i < k; i++)
        parts.push(`<gen-c${i} /><button on:click="a${i}($event)">{v${i}}</button>`);
    return `<main>${parts.join('')}</main>`;
}

describe('deterministic ids (default) are reproducible and collision-free', () => {
    test('same source compiles to byte-identical output across two runs', async () => {
        const html = bigPage(50);
        expect(await compile(html, '/pages/p/tac.js')).toBe(await compile(html, '/pages/p/tac.js'));
    });

    test('no id collisions within a page, even at 2000 components + 2000 handlers', async () => {
        const k = 2000;
        const code = await compile(bigPage(k), '/pages/big/tac.js');
        const hostIds = new Set([...code.matchAll(/__tc_host_([a-z0-9]+)\b/g)].map((m) => m[1]));
        const eventIds = new Set([...code.matchAll(/tc_invokeEvent\('([a-z0-9]+)'/g)].map((m) => m[1]));
        // One unique host id per component and one unique event id per handler —
        // a collision would drop the distinct count below k.
        expect(hostIds.size).toBe(k);
        expect(eventIds.size).toBe(k);
        // And the two spaces never overlap (disjoint \0comp / \0tpl prefixes).
        expect([...hostIds].some((id) => eventIds.has(id))).toBe(false);
    });

    test('different modules get disjoint id prefixes (no cross-page collision)', async () => {
        // Identical source, different module identity → different ids, so a wrapper
        // embedding a nested page never collides ids in the combined document.
        const a = new Set(idsIn(await compile(bigPage(30), '/pages/a/tac.js')));
        const b = new Set(idsIn(await compile(bigPage(30), '/pages/b/tac.js')));
        expect([...a].some((id) => b.has(id))).toBe(false);
    });

    test('element/event ids and component-host ids never overlap in one module', async () => {
        // parseHTML mints event ids; createJSData mints host ids. The \0tpl / \0comp
        // seed tags must keep those two spaces disjoint.
        const code = await compile(bigPage(100), '/pages/mix/tac.js');
        const hostIds = new Set([...code.matchAll(/__tc_host_([a-z0-9]+)\b/g)].map((m) => m[1]));
        const eventIds = new Set([...code.matchAll(/tc_invokeEvent\('([a-z0-9]+)'/g)].map((m) => m[1]));
        expect([...hostIds].some((id) => eventIds.has(id))).toBe(false);
    });

    test('TAC_RANDOM_IDS escape hatch restores non-reproducible ids', async () => {
        const previous = process.env.TAC_RANDOM_IDS;
        process.env.TAC_RANDOM_IDS = '1';
        try {
            const html = bigPage(10);
            expect(await compile(html, '/pages/r/tac.js')).not.toBe(await compile(html, '/pages/r/tac.js'));
        }
        finally {
            if (previous === undefined) delete process.env.TAC_RANDOM_IDS;
            else process.env.TAC_RANDOM_IDS = previous;
        }
    });
});
