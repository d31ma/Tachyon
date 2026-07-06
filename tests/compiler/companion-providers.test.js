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
    test('exposes only the TS/JS and Rust Tac worker languages', () => {
        expect(Compiler.workerProviders).toEqual([
            { extension: '.rs', language: 'rust', targets: ['web', 'macos', 'windows', 'linux', 'ios', 'android'] },
            { extension: '.js', language: 'javascript', targets: ['web'] },
            { extension: '.ts', language: 'typescript', targets: ['web'] },
        ]);

        expect(Compiler.workerSubsetLanguages().sort()).toEqual([
            'javascript',
            'rust',
            'typescript',
        ]);
        expect(Compiler.subsetLanguages().sort()).toEqual(['javascript', 'rust', 'typescript']);

        // Everything else — including the previously-supported C#, C++, Swift,
        // Kotlin — is no longer a worker language.
        for (const extension of ['.c', '.cpp', '.cs', '.go', '.py', '.zig', '.swift', '.kt']) {
            expect(Compiler.getWorkerProvider(`/tmp/tac${extension}`)).toBeNull();
        }
    });

    test('discovers JavaScript companions through provider metadata', async () => {
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-provider-'));
        tempDirs.push(root);
        await mkdir(path.join(root, 'client', 'components', 'card'), { recursive: true });
        const templatePath = path.join(root, 'client', 'components', 'card', 'tac.html');
        const companionPath = path.join(root, 'client', 'components', 'card', 'tac.js');
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
        await mkdir(path.join(root, 'client', 'components', 'card'), { recursive: true });
        const templatePath = path.join(root, 'client', 'components', 'card', 'tac.html');
        const companionPath = path.join(root, 'client', 'components', 'card', 'tac.ts');
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
        await mkdir(path.join(root, 'client', 'components', 'raw'), { recursive: true });
        const templatePath = path.join(root, 'client', 'components', 'raw', 'tac.html');
        await writeFile(templatePath, '<span>{label}</span>');
        await writeFile(path.join(root, 'client', 'components', 'raw', 'tac.rs'), '/* rust source */');
        await writeFile(path.join(root, 'client', 'components', 'raw', 'tac.cpp'), '/* cpp source */');

        expect(await Compiler.getCompanionScript(templatePath)).toBeNull();
        expect(await Compiler.getCompanionScriptPath(templatePath)).toBeNull();
    });

    test('returns null when no provider matches a companion file', async () => {
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-provider-'));
        tempDirs.push(root);
        await mkdir(path.join(root, 'client', 'components', 'panel'), { recursive: true });
        const templatePath = path.join(root, 'client', 'components', 'panel', 'tac.html');
        await writeFile(templatePath, '<section>Panel</section>');

        expect(await Compiler.getCompanionScript(templatePath)).toBeNull();
        expect(await Compiler.getCompanionScriptPath(templatePath)).toBeNull();
    });

    test('detects app-authored tac protocol fetch usage for automatic shadow imports', () => {
        expect(Compiler.referencesTacFetch(`
            export default class extends Tac {
                async run() {
                    return fetch('tac://language', { method: 'POST' })
                }
            }
        `)).toBe(true);
    });

    test('does not inject the tac fetch shadow when fetch is app-owned or no tac url is used', () => {
        // App-declared fetch should win over the injected shadow.
        expect(Compiler.referencesTacFetch(`
            import { fetch } from './custom-fetch.js'
            export default class extends Tac {
                async run() { return fetch('tac://language') }
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
