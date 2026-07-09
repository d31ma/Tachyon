// @ts-check
import { describe, expect, test } from 'bun:test';
import Compiler from '../../src/compiler/index.js';

/** Run the full render-source pass (where control directives are compiled). */
async function render(html) {
    const nodes = await Compiler.parseHTML(html, 'diag-test');
    return Compiler.createJSData({ script: '', scriptLang: 'js' }, nodes, '/pages/tac.js');
}

/** Compile just the template body (where interpolation is validated). */
async function compileBody(html) {
    const nodes = await Compiler.parseHTML(html, 'diag-test');
    return nodes.map((n) => n.element).filter(Boolean).join('\n');
}

describe('malformed <loop>/<logic> directives fail the build instead of leaking markup', () => {
    test('a well-formed <loop :for> compiles', async () => {
        expect(await render('<loop :for="x of xs"><li>{x}</li></loop>')).toContain('for(let x of xs)');
    });

    test('<loop> with a colon-less for= is a named error, not a silent leak', async () => {
        await expect(render('<loop for="x of xs"><li>x</li></loop>'))
            .rejects.toThrow(/<loop>.*expected a :for directive/s);
    });

    test('a well-formed <logic :if> compiles', async () => {
        expect(await render('<logic :if="ok"><p>x</p></logic>')).toContain('if(ok)');
    });

    test('<logic> with a typo\'d directive is a named error', async () => {
        await expect(render('<logic :iff="ok"><p>x</p></logic>'))
            .rejects.toThrow(/<logic>.*expected :if, :else-if, or else/s);
    });
});

describe('interpolation expressions are syntax-checked at build time', () => {
    test('a valid expression compiles', async () => {
        expect(await compileBody('<p>{ user.name }</p>')).toContain('tc_escapeText(user.name)');
    });

    test('a syntactically broken expression is a named error', async () => {
        await expect(compileBody('<p>{ user.nam( }</p>'))
            .rejects.toThrow(/Invalid Tac interpolation.*not a valid expression/s);
    });

    test('a broken raw { ! ... } expression is also caught', async () => {
        await expect(compileBody('<p>{! foo(( }</p>'))
            .rejects.toThrow(/not a valid expression/);
    });

    test('await is permitted inside an interpolation', async () => {
        expect(await compileBody('<p>{ await load() }</p>')).toContain('tc_escapeText(await load())');
    });
});
