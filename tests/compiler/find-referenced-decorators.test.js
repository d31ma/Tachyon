// @ts-check
import { describe, expect, test } from 'bun:test';
import Compiler from '../../src/compiler/index.js';

describe('Compiler.findReferencedDecorators', () => {
    test('detects each supported decorator', () => {
        const source = `
            export default class extends Tac {
              @subscribe('k') a
              @publish('k') b = 1
              @env('PORT') c
              @onMount run() {}
            }
        `;
        expect(Compiler.findReferencedDecorators(source).sort())
            .toEqual(['env', 'onMount', 'publish', 'subscribe']);
    });

    test('detects bare supported decorators', () => {
        const source = `
            export default class extends Tac {
              @subscribe a
              @publish b = 1
            }
        `;
        expect(Compiler.findReferencedDecorators(source).sort())
            .toEqual(['publish', 'subscribe']);
    });

    test('returns empty list when no decorators are used', () => {
        const source = `export default class extends Tac { x = 1 }`;
        expect(Compiler.findReferencedDecorators(source)).toEqual([]);
    });

    test('skips decorators that are already imported', () => {
        const source = `
            import { subscribe } from './local.js'
            export default class extends Tac {
              @subscribe('k') a
              @publish('k') b = 1
            }
        `;
        expect(Compiler.findReferencedDecorators(source)).toEqual(['publish']);
    });

    test('skips decorators that are locally declared', () => {
        const source = `
            const subscribe = () => () => {}
            export default class extends Tac {
              @subscribe('k') a
              @publish('k') b = 1
            }
        `;
        expect(Compiler.findReferencedDecorators(source)).toEqual(['publish']);
    });

    test('injects the auto-import when an unrelated import precedes a later `from` (#108)', () => {
        // A top-level import plus a later `.from` used to make the loose
        // detection regex span from the import, past it, to the @onMount usage
        // and the stray `from`, wrongly treating onMount as already imported —
        // silently dropping the auto-import on larger companions.
        const source = `
            import { helper } from '../../shared.js'
            export default class extends Tac {
              items = [1, 2, 3]
              @onMount
              async boot() {
                const rows = Array.from(this.items)
                this.status = helper(rows.length)
              }
            }
        `;
        expect(Compiler.findReferencedDecorators(source)).toEqual(['onMount']);
    });

    test('still skips a genuine onMount import alongside an unrelated one', () => {
        const source = `
            import { helper } from './util.js'
            import { onMount } from './my-decorators.js'
            export default class extends Tac {
              @onMount boot() { return Array.from([]) }
            }
        `;
        expect(Compiler.findReferencedDecorators(source)).toEqual([]);
    });

    test('does not match JSDoc-style mentions', () => {
        const source = `
            /**
             * Implements a @subscribe pattern. See @publish note.
             */
            export default class extends Tac {}
        `;
        expect(Compiler.findReferencedDecorators(source)).toEqual([]);
    });

    test('does not match email-style or partial-word matches', () => {
        const source = `
            // contact: foo@subscribe.example.com
            const resubscribe = 1
            export default class extends Tac {
              raw = '@subscription'
            }
        `;
        expect(Compiler.findReferencedDecorators(source)).toEqual([]);
    });

    test('rejects removed v2 decorators with a migration hint', () => {
        const source = `
            export default class extends Tac {
              @render
              update() {}
            }
        `;
        expect(() => Compiler.assertNoRemovedDecorators(source, '/app/client/components/card/tac.js'))
            .toThrow('Tac decorator @render is not supported in v2. Reassign instance fields to trigger automatic rerenders instead.');
    });

    test('allows app-owned symbols with removed decorator names', () => {
        const source = `
            import { render } from './local-render.js'
            export default class extends Tac {
              @render
              update() {}
            }
        `;
        expect(() => Compiler.assertNoRemovedDecorators(source, '/app/client/components/card/tac.js')).not.toThrow();
    });
});
