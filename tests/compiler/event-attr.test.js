// @ts-check
import { describe, expect, test } from 'bun:test';
import Compiler from '../../src/compiler/index.js';

/** Compile a template fragment and return the generated render-source string. */
async function compile(html) {
    const nodes = await Compiler.parseHTML(html, 'event-attr-test');
    return nodes.map((node) => node.element ?? node.static ?? '').join('\n');
}

describe('on:<event> compiles to a DuVay-safe marker (no literal on:<event> in the DOM)', () => {
    test('a simple custom event becomes data-tac-on-<event>', async () => {
        const out = await compile('<w-confirm-edit on:save="save($event)">x</w-confirm-edit>');
        expect(out).toContain('data-tac-on-save="');
        expect(out).not.toMatch(/on:save=/);
        // The handler closure is still wired through tc_invokeEvent.
        expect(out).toContain('tc_invokeEvent(');
        expect(out).toContain('return (save($event));');
    });

    test('a colon event name encodes the inner colon as __', async () => {
        const out = await compile('<w-data-table on:update:selected="pick($event)">x</w-data-table>');
        expect(out).toContain('data-tac-on-update__selected="');
        expect(out).not.toMatch(/on:update:selected=/);
        // Encoding must not collide with the :binding rewrite (no stray selected="...").
        expect(out).not.toMatch(/(^|\s)selected="\$\{tc_escapeAttr/);
    });

    test('the colon-free alias stays a plain dashed marker', async () => {
        const out = await compile('<w-data-table on:update-selected="pick($event)">x</w-data-table>');
        expect(out).toContain('data-tac-on-update-selected="');
        expect(out).not.toContain('data-tac-on-update__selected');
    });

    test('a bare on* attribute (no colon) is NOT an event — props like onboarding survive', async () => {
        const out = await compile('<x-el onboarding="yes" online="true"></x-el>');
        expect(out).toContain('onboarding="yes"');
        expect(out).toContain('online="true"');
        expect(out).not.toContain('data-tac-on-boarding');
        expect(out).not.toContain('data-tac-on-line');
    });

    test('@chex passes through untouched (owned by component libraries, not Tac)', async () => {
        const out = await compile('<input @chex="^[0-9]+$" />');
        // `@chex` is not an event binding (only `on:<event>` is). It flows through as
        // a plain attribute so DuVay can read it.
        expect(out).toContain('@chex="^[0-9]+$"');
        expect(out).not.toContain('data-tac-on-chex');
    });
});

describe('a bare native handler is a footgun — warn and suggest the on: directive', () => {
    /** Compile `html`, returning the warnings the compiler logged. */
    async function warningsFor(html) {
        Compiler.warnedBareHandlers.clear();
        const original = Compiler.compilerLogger.warn;
        /** @type {Array<{ message: string, meta: any }>} */
        const warnings = [];
        Compiler.compilerLogger.warn = (message, meta) => warnings.push({ message, meta });
        try {
            await compile(html);
        }
        finally {
            Compiler.compilerLogger.warn = original;
        }
        return warnings;
    }

    test('onclick warns and points at on:click, while still emitting the raw attribute', async () => {
        const warnings = await warningsFor('<button onclick="save()">x</button>');
        expect(warnings).toHaveLength(1);
        expect(warnings[0].message).toContain('on:click');
        expect(warnings[0].meta.suggestion).toBe('on:click');
        // Behavior is unchanged — the attribute still flows through untouched.
        expect(await compile('<button onclick="save()">x</button>')).toContain('onclick="save()"');
    });

    test('onboarding/online do not warn (real attributes, not handlers)', async () => {
        expect(await warningsFor('<x-el onboarding="yes" online="true"></x-el>')).toHaveLength(0);
    });

    test('the on: directive form does not warn', async () => {
        expect(await warningsFor('<button on:click="save($event)">x</button>')).toHaveLength(0);
    });
});

describe('on:<event> handler values must be a single expression', () => {
    test('a multi-statement handler fails with a clear, attribute-named error', async () => {
        await expect(compile('<button on:load="var x = 1; doThing()">x</button>'))
            .rejects.toThrow(/on:load handler must be a single expression/);
    });

    test('a bare block value fails with the single-expression error', async () => {
        await expect(compile('<button on:click="{ foo() }">x</button>'))
            .rejects.toThrow(/single expression/);
    });

    test('a plain method-call expression compiles cleanly', async () => {
        const out = await compile('<button on:click="increment($event)">x</button>');
        expect(out).toContain('data-tac-on-click="');
        expect(out).toContain('return (increment($event));');
    });
});

describe('compile-time event set is registered once (no per-render DOM scan)', () => {
    /** @returns {Promise<string[]>} the event names passed to the injected delegateEvents call */
    async function eventSetOf(html) {
        const nodes = await Compiler.parseHTML(html, 'event-set-test');
        const code = await Compiler.createJSData({ script: '', scriptLang: 'js' }, nodes, '/pages/tac.js');
        const match = code.match(/__tc_helpers__\.delegateEvents\(\[([^\]]*)\]\)/);
        if (!match)
            return [];
        return [...match[1].matchAll(/\\?"([^"\\]+)\\?"/g)].map((m) => m[1]);
    }

    test('collects the distinct event types used across the template', async () => {
        const events = await eventSetOf(`
            <main>
                <button on:click="a($event)">a</button>
                <button on:click="b($event)">b</button>
                <w-confirm-edit on:save="save($event)" on:cancel="cancel($event)" />
            </main>`);
        expect(events.sort()).toEqual(['cancel', 'click', 'save']);
    });

    test('decodes colon event names back from the marker encoding', async () => {
        const events = await eventSetOf('<w-data-table on:update:selected="pick($event)" />');
        expect(events).toContain('update:selected');
    });

    test('a template with no handlers injects no delegateEvents call', async () => {
        const nodes = await Compiler.parseHTML('<main><p>static</p></main>', 'no-events');
        const code = await Compiler.createJSData({ script: '', scriptLang: 'js' }, nodes, '/pages/tac.js');
        expect(code).not.toContain('__tc_helpers__.delegateEvents([');
    });
});

describe('component invocations register for scoped re-render', () => {
    test('emits a hoisted host id and a registerComponentRender call', async () => {
        Compiler.compMapping.set('counter', 'counter/tac.js');
        try {
            const nodes = await Compiler.parseHTML('<main><counter /></main>', 'comp-scope-test');
            const code = await Compiler.createJSData({ script: '', scriptLang: 'js' }, nodes, '/pages/tac.js');
            // Host id hoisted into a const, used as the div id and the registry key.
            expect(code).toMatch(/const __tc_host_[a-z0-9]+ = tc_generateId\('[a-z0-9]+', 'id'\)/);
            expect(code).toMatch(/__tc_helpers__\.registerComponentRender\(__tc_host_[a-z0-9]+, render, '[a-z0-9]+'\)/);
        }
        finally {
            Compiler.compMapping.delete('counter');
        }
    });
});

describe('component :props pass live values, not stringified attributes', () => {
    /** @returns {Promise<string>} the generated child-props object literal */
    async function propsOf(html) {
        Compiler.compMapping.set('counter', 'counter/tac.js');
        try {
            const nodes = await Compiler.parseHTML(html, 'prop-test');
            // The factory source is embedded via JSON.stringify, so quotes are
            // backslash-escaped in the returned module code — unescape before matching.
            const code = (await Compiler.createJSData({ script: '', scriptLang: 'js' }, nodes, '/pages/tac.js')).replaceAll('\\"', '"');
            return (code.match(/const __tc_child_props_\w+ = (\{[\s\S]*?\})/) || [])[1] || '';
        }
        finally {
            Compiler.compMapping.delete('counter');
        }
    }

    test('a dynamic array prop is passed as a live array (not tc_escapeAttr-stringified)', async () => {
        const props = await propsOf('<counter :items="[1, 2, 3]" />');
        expect(props).toContain('"items": [1, 2, 3]');
        expect(props).not.toContain('tc_escapeAttr');
    });

    test('a dynamic object prop is passed as a live object', async () => {
        const props = await propsOf(`<counter :config="{ a: 1, b: 'x' }" />`);
        expect(props).toContain(`"config": { a: 1, b: 'x' }`);
    });

    test('a dynamic call expression is unwrapped (handles nested parens)', async () => {
        const props = await propsOf('<counter :rows="fetchRows(page)" />');
        expect(props).toContain('"rows": fetchRows(page)');
    });

    test('a plain identifier prop stays a live reference', async () => {
        const props = await propsOf('<counter :label="title" />');
        expect(props).toContain('"label": title');
    });

    test('a static attribute prop stays a JSON string literal', async () => {
        const props = await propsOf(`<counter items='[1,2,3]' />`);
        expect(props).toContain('"items": "[1,2,3]"');
    });

    test('a string value with embedded HTML is passed raw (escaping happens at render, not the prop boundary)', async () => {
        const props = await propsOf(`<counter :note="'<b>x</b> & y'" />`);
        // The raw string reaches the child verbatim — NOT tc_escapeAttr'd here.
        expect(props).toContain(`"note": '<b>x</b> & y'`);
        expect(props).not.toContain('tc_escapeAttr');
        // And an interpolation of that value compiles to an escaping call, so it
        // renders as escaped text (proven end-to-end in bundle-static-export).
        const nodes = await Compiler.parseHTML('<span>{note}</span>', 'esc');
        const code = await Compiler.createJSData({ script: '', scriptLang: 'js' }, nodes, '/pages/tac.js');
        expect(code).toContain('tc_escapeText(note)');
    });
});
