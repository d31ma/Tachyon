// @ts-check
import { afterEach, describe, expect, test } from 'bun:test';
import { mkdir, mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import Compiler from '../../src/compiler/index.js';

/** @type {string[]} */
const tempDirs = [];

afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

describe('Compiler companion providers', () => {
    test('discovers JavaScript companions through provider metadata', async () => {
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-provider-'));
        tempDirs.push(root);
        await mkdir(path.join(root, 'components', 'card'), { recursive: true });
        const templatePath = path.join(root, 'components', 'card', 'tac.html');
        const companionPath = path.join(root, 'components', 'card', 'tac.js');
        await writeFile(templatePath, '<article>{title}</article>');
        await writeFile(companionPath, 'export default class {}');

        const companion = await Compiler.getCompanionScript(templatePath);

        expect(companion?.sourcePath).toBe(companionPath);
        expect(companion?.importPath).toBe('./tac.js');
        expect(companion?.provider).toEqual({ extension: '.js', target: 'ecmascript' });
        expect(await Compiler.getCompanionScriptPath(templatePath)).toBe(companionPath);
    });

    test('falls back to TypeScript companions when JavaScript is absent', async () => {
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-provider-'));
        tempDirs.push(root);
        await mkdir(path.join(root, 'components', 'card'), { recursive: true });
        const templatePath = path.join(root, 'components', 'card', 'tac.html');
        const companionPath = path.join(root, 'components', 'card', 'tac.ts');
        await writeFile(templatePath, '<article>{title}</article>');
        await writeFile(companionPath, 'export default class {}');

        const companion = await Compiler.getCompanionScript(templatePath);

        expect(companion?.sourcePath).toBe(companionPath);
        expect(companion?.importPath).toBe('./tac.ts');
        expect(companion?.provider).toEqual({ extension: '.ts', target: 'ecmascript' });
    });

    test('ignores removed non-JavaScript companion formats', async () => {
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-provider-'));
        tempDirs.push(root);
        await mkdir(path.join(root, 'components', 'raw'), { recursive: true });
        const templatePath = path.join(root, 'components', 'raw', 'tac.html');
        await writeFile(templatePath, '<span>{label}</span>');
        await writeFile(path.join(root, 'components', 'raw', 'tac.rs'), '/* rust source */');
        await writeFile(path.join(root, 'components', 'raw', 'tac.cpp'), '/* cpp source */');

        expect(await Compiler.getCompanionScript(templatePath)).toBeNull();
        expect(await Compiler.getCompanionScriptPath(templatePath)).toBeNull();
    });

    test('returns null when no provider matches a companion file', async () => {
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-provider-'));
        tempDirs.push(root);
        await mkdir(path.join(root, 'components', 'panel'), { recursive: true });
        const templatePath = path.join(root, 'components', 'panel', 'tac.html');
        await writeFile(templatePath, '<section>Panel</section>');

        expect(await Compiler.getCompanionScript(templatePath)).toBeNull();
        expect(await Compiler.getCompanionScriptPath(templatePath)).toBeNull();
    });

    test('detects app-authored tac protocol fetch usage for automatic shadow imports', () => {
        expect(Compiler.referencesTacFetch(`
            export default class extends Tac {
                async run() {
                    return fetch('tac://language/rust', { method: 'POST' })
                }
            }
        `)).toBe(true);
    });

    test('does not inject the tac fetch shadow when fetch is app-owned or no tac url is used', () => {
        // App-declared fetch should win over the injected shadow.
        expect(Compiler.referencesTacFetch(`
            import { fetch } from './custom-fetch.js'
            export default class extends Tac {
                async run() { return fetch('tac://language/rust') }
            }
        `)).toBe(false);
        // Plain fetch without any tac:// URL must not trigger injection.
        expect(Compiler.referencesTacFetch(`
            export default class extends Tac {
                async run() { return fetch('/api/data') }
            }
        `)).toBe(false);
    });
});
