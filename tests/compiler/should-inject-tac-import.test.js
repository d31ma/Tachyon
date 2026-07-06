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

describe('Compiler.injectTacBaseClass', () => {
    test('adds extends Tac to an anonymous default companion class', () => {
        expect(Compiler.injectTacBaseClass(`export default class {\n  count = 0\n}`))
            .toBe(`export default class extends Tac {\n  count = 0\n}`);
    });

    test('adds extends Tac to a named default companion class', () => {
        expect(Compiler.injectTacBaseClass(`export default class Clicker {\n  count = 0\n}`))
            .toBe(`export default class Clicker extends Tac {\n  count = 0\n}`);
    });

    test('supports TypeScript generic class declarations', () => {
        const source = `export default class Store<TValue> {\n  value: TValue | null = null\n}`;
        expect(Compiler.injectTacBaseClass(source))
            .toBe(`export default class Store<TValue> extends Tac {\n  value: TValue | null = null\n}`);
    });

    test('adds a super call when the companion constructor is author-defined', () => {
        const source = [
            `export default class {`,
            `  constructor() {`,
            `    this.count = 1`,
            `  }`,
            `}`,
        ].join('\n');
        expect(Compiler.injectTacBaseClass(source)).toBe([
            `export default class extends Tac {`,
            `  constructor() {`,
            `      super(...arguments);`,
            `    this.count = 1`,
            `  }`,
            `}`,
        ].join('\n'));
    });

    test('does not duplicate an existing super call', () => {
        const source = [
            `export default class {`,
            `  constructor(props) {`,
            `    super(props)`,
            `  }`,
            `}`,
        ].join('\n');
        expect(Compiler.injectTacBaseClass(source)).toBe([
            `export default class extends Tac {`,
            `  constructor(props) {`,
            `    super(props)`,
            `  }`,
            `}`,
        ].join('\n'));
    });

    test('does not change classes that already extend a base class', () => {
        const source = `export default class extends ViewModel {\n  count = 0\n}`;
        expect(Compiler.injectTacBaseClass(source)).toBe(source);
    });

    test('ignores class-looking text in comments and strings', () => {
        const source = [
            `// export default class {}`,
            `const note = "export default class {"`,
            `export default class {}`,
        ].join('\n');
        expect(Compiler.injectTacBaseClass(source)).toBe([
            `// export default class {}`,
            `const note = "export default class {"`,
            `export default class extends Tac {}`,
        ].join('\n'));
    });
});
