// @ts-check
import { describe, expect, test } from 'bun:test';
import Compiler from '../../src/compiler/index.js';

describe('Compiler.referencesFyloGlobal', () => {
    test('detects bare property access on fylo', () => {
        const source = `
            export default class extends Tac {
                async load() {
                    return await fylo.users.find({ $ops: [] });
                }
            }
        `;
        expect(Compiler.referencesFyloGlobal(source)).toBe(true);
    });

    test('detects fylo.sql calls', () => {
        const source = `
            export default class extends Tac {
                async load() {
                    return await fylo.sql('SELECT * FROM users');
                }
            }
        `;
        expect(Compiler.referencesFyloGlobal(source)).toBe(true);
    });

    test('returns false when fylo is not referenced', () => {
        const source = `export default class extends Tac { x = 1 }`;
        expect(Compiler.referencesFyloGlobal(source)).toBe(false);
    });

    test('skips when fylo is already imported', () => {
        const source = `
            import { fylo } from './local.js'
            export default class extends Tac {
                load() { return fylo.users.find() }
            }
        `;
        expect(Compiler.referencesFyloGlobal(source)).toBe(false);
    });

    test('skips when fylo is locally declared', () => {
        const source = `
            const fylo = () => null
            export default class extends Tac {
                load() { return fylo.users.find() }
            }
        `;
        expect(Compiler.referencesFyloGlobal(source)).toBe(false);
    });

    test('does not match dotted member access (e.g. some.fylo)', () => {
        const source = `
            const obj = { other: 1 }
            export default class extends Tac {
                read() { return obj.fylo }
            }
        `;
        expect(Compiler.referencesFyloGlobal(source)).toBe(false);
    });

    test('does not match strings or comments', () => {
        const source = `
            // calling fylo.users.find()
            /** @returns whatever fylo */
            export default class extends Tac {
                msg = 'fylo.users.find()'
            }
        `;
        expect(Compiler.referencesFyloGlobal(source)).toBe(false);
    });

    test('does not match identifiers that contain "fylo"', () => {
        const source = `
            const myfylo = 1
            const fyloRaw = 2
            export default class extends Tac { x = myfylo + fyloRaw }
        `;
        expect(Compiler.referencesFyloGlobal(source)).toBe(false);
    });
});
