// @ts-check
import { describe, expect, test } from 'bun:test';
import Compiler from '../../src/compiler/index.js';

describe('Compiler.shouldInjectTacImport', () => {
    test('returns true when Tac is not imported or declared', () => {
        expect(Compiler.shouldInjectTacImport(`export default class extends Tac {}`))
            .toBe(true);
    });

    test('returns false when import Tac from is already present', () => {
        expect(Compiler.shouldInjectTacImport(`import Tac from './tac.js';\nexport default class extends Tac {}`))
            .toBe(false);
    });

    test('returns false when class Tac is locally declared', () => {
        expect(Compiler.shouldInjectTacImport(`class Tac {}\nexport default class extends Tac {}`))
            .toBe(false);
    });

    test('returns false when var Tac is locally declared', () => {
        expect(Compiler.shouldInjectTacImport(`var Tac = class {};\nexport default class extends Tac {}`))
            .toBe(false);
    });

    test('returns true when Array.from appears but no Tac import (regression for #57)', () => {
        const source = [
            `import { onMount } from './decorators.js';`,
            `export default class extends Tac {`,
            `  get folders() {`,
            `    return Array.from(['inbox', 'archive']);`,
            `  }`,
            `}`,
        ].join('\n');
        expect(Compiler.shouldInjectTacImport(source)).toBe(true);
    });

    test('returns true when Uint8Array.from appears but no Tac import', () => {
        const source = [
            `import { onMount } from './decorators.js';`,
            `export default class extends Tac {`,
            `  decode(data) {`,
            `    const bytes = Uint8Array.from(atob(data.contentBase64), c => c.charCodeAt(0));`,
            `    return bytes;`,
            `  }`,
            `}`,
        ].join('\n');
        expect(Compiler.shouldInjectTacImport(source)).toBe(true);
    });

    test('returns true when Tac appears in a comment with from on a later line', () => {
        const source = [
            `// This component extends Tac, imported from the framework`,
            `export default class extends Tac {`,
            `  foo = 'bar';`,
            `}`,
        ].join('\n');
        expect(Compiler.shouldInjectTacImport(source)).toBe(true);
    });

    test('returns true when Tac and from appear in string literals', () => {
        const source = [
            `const msg = "extends Tac from the base class";`,
            `const note = 'Array.from is faster';`,
            `export default class extends Tac {}`,
        ].join('\n');
        expect(Compiler.shouldInjectTacImport(source)).toBe(true);
    });

    test('returns false when Tac is a named import', () => {
        expect(Compiler.shouldInjectTacImport(`import { Tac } from './utils.js';\nexport default class extends Tac {}`))
            .toBe(false);
    });

    test('returns true when import without semicolon does not reference Tac (TS no-semicolon)', () => {
        const source = [
            `import dayjs from 'dayjs'`,
            `type InventoryItem = {`,
            `    id: string`,
            `    name: string`,
            `}`,
            `export default class extends Tac {`,
            `    items: InventoryItem[] = []`,
            `}`,
        ].join('\n');
        expect(Compiler.shouldInjectTacImport(source)).toBe(true);
    });

    test('returns false when import type Tac is present', () => {
        expect(Compiler.shouldInjectTacImport(`import type Tac from './tac.js';\nexport default class extends Tac {}`))
            .toBe(false);
    });
});
