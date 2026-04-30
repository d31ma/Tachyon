// @ts-check
import { describe, expect, test } from 'bun:test';
import Compiler from '../../src/compiler/index.js';

describe('Compiler.findReferencedDecorators', () => {
    test('detects each supported decorator', () => {
        const source = `
            export default class extends Tac {
              @inject('k') a
              @provide('k') b = 1
              @env('PORT') c
              @onMount run() {}
              @emit('x') save() {}
            }
        `;
        expect(Compiler.findReferencedDecorators(source).sort())
            .toEqual(['emit', 'env', 'inject', 'onMount', 'provide']);
    });

    test('returns empty list when no decorators are used', () => {
        const source = `export default class extends Tac { x = 1 }`;
        expect(Compiler.findReferencedDecorators(source)).toEqual([]);
    });

    test('skips decorators that are already imported', () => {
        const source = `
            import { inject } from './local.js'
            export default class extends Tac {
              @inject('k') a
              @provide('k') b = 1
            }
        `;
        expect(Compiler.findReferencedDecorators(source)).toEqual(['provide']);
    });

    test('skips decorators that are locally declared', () => {
        const source = `
            const inject = () => () => {}
            export default class extends Tac {
              @inject('k') a
              @provide('k') b = 1
            }
        `;
        expect(Compiler.findReferencedDecorators(source)).toEqual(['provide']);
    });

    test('does not match JSDoc-style mentions', () => {
        const source = `
            /**
             * Implements an @inject pattern. See @provide note.
             */
            export default class extends Tac {}
        `;
        expect(Compiler.findReferencedDecorators(source)).toEqual([]);
    });

    test('does not match email-style or partial-word matches', () => {
        const source = `
            // contact: foo@inject.example.com
            const reinject = 1
            export default class extends Tac {
              raw = '@injection'
            }
        `;
        expect(Compiler.findReferencedDecorators(source)).toEqual([]);
    });
});
