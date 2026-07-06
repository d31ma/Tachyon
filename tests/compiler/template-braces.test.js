// @ts-check
import { expect, test } from 'bun:test';
import Compiler from '../../src/compiler/index.js';

/** @param {string} html */
async function compileBody(html) {
    const nodes = await Compiler.parseHTML(html);
    return nodes.map((n) => n.element).filter(Boolean).join('\n');
}

test('backslash-escaped braces emit literal { } in text, not interpolation', async () => {
    const out = await compileBody('<p>shape is \\{ a, b \\} nodes</p>');
    expect(out).toContain('shape is { a, b } nodes');
    // No JS expression was generated from the escaped braces.
    expect(out).not.toContain('tc_escapeText(a, b)');
});

test('escaped braces coexist with live interpolation', async () => {
    const out = await compileBody('<p>{ live } in \\{ braces \\}</p>');
    expect(out).toContain('${tc_escapeText(live)}');
    expect(out).toContain('in { braces }');
});

test('raw { ! ... } interpolation still works', async () => {
    const out = await compileBody('<p>{! html }</p>');
    expect(out).toContain('${html}');
});

test('raw double-quotes in attribute values are entity-escaped, not truncated', async () => {
    const out = await compileBody(`<x-el data='[{"k":"v"}]'></x-el>`);
    expect(out).toContain('data="[{&quot;k&quot;:&quot;v&quot;}]"');
    // The naked-quote serialization that broke the attribute must be gone.
    expect(out).not.toContain('data="[{"k":"v"}]"');
});
