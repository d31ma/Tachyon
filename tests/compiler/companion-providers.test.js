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
    test('exposes portable companion providers instead of a client worker API', () => {
        expect(Compiler.companionProviders).toEqual([
            { extension: '.js', language: 'javascript', target: 'ecmascript', portable: true },
            { extension: '.ts', language: 'typescript', target: 'ecmascript', portable: true },
            { extension: '.dart', language: 'dart', target: 'dart', portable: true },
            { extension: '.rs', language: 'rust', target: 'subset', portable: true },
            { extension: '.kt', language: 'kotlin', target: 'subset', portable: true },
            { extension: '.swift', language: 'swift', target: 'subset', portable: true },
            { extension: '.cs', language: 'csharp', target: 'subset', portable: true },
        ]);
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
        expect(companion?.provider).toEqual({ extension: '.js', language: 'javascript', target: 'ecmascript', portable: true });
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
        expect(companion?.provider).toEqual({ extension: '.ts', language: 'typescript', target: 'ecmascript', portable: true });
    });

    test('discovers Dart companions when no JavaScript companion is present', async () => {
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-dart-companion-provider-'));
        tempDirs.push(root);
        await mkdir(path.join(root, 'client', 'components', 'counter'), { recursive: true });
        const templatePath = path.join(root, 'client', 'components', 'counter', 'tac.html');
        const companionPath = path.join(root, 'client', 'components', 'counter', 'tac.dart');
        await writeFile(templatePath, '<article>{count}</article>');
        await writeFile(companionPath, 'class Counter extends Tac { int count = 0; }');

        const companion = await Compiler.getCompanionScript(templatePath);

        expect(companion?.sourcePath).toBe(companionPath);
        expect(companion?.importPath).toBe('./tac.dart');
        expect(companion?.provider).toEqual({ extension: '.dart', language: 'dart', target: 'dart', portable: true });
    });

    test('treats every supported companion language as portable across bundle targets', () => {
        for (const provider of Compiler.companionProviders) {
            for (const target of ['web', 'macos', 'windows', 'linux', 'ios', 'android']) {
                expect(Compiler.companionProviderSupportsTarget(provider, target)).toBe(true);
            }
        }
    });

    test('lowers JavaScript implicit platform APIs without requiring a Tac API in source', () => {
        const output = Compiler.lowerJavaScriptNativeShims(`
export default class {
    async inspect() {
        const info = await app.info()
        await browser.open('https://tachyon.del.ma')
        await clipboard.writeText('ready')
        await fileSystem.writeText('/tmp/ready.txt', 'ready')
        await shell.exec('echo', ['ready'])
        await share.text('ready')
        await haptics.impact()
        await shortcuts.register({ id: 'toggle', accelerator: 'Primary+T' })
        await appWindow.setOpacity(0.8)
        await contentSurface.open({ id: 'docs', url: 'https://example.com' })
        await screenCapture.listWindows()
        await host.invoke('example.echo', { value: 'ready' })
        return capabilities.supports('app.info') ? info : null
    }
}
`);

        expect(output).toContain('await this.tac.__native.app.info()');
        expect(output).toContain("await this.tac.__native.browser.open('https://tachyon.del.ma')");
        expect(output).toContain("await this.tac.__native.clipboard.writeText('ready')");
        expect(output).toContain("await this.tac.__native.fileSystem.writeText('/tmp/ready.txt', 'ready')");
        expect(output).toContain("await this.tac.__native.shell.exec('echo', ['ready'])");
        expect(output).toContain("await this.tac.__native.share.text('ready')");
        expect(output).toContain('await this.tac.__native.haptics.impact()');
        expect(output).toContain('await this.tac.__native.shortcuts.register(');
        expect(output).toContain('await this.tac.__native.appWindow.setOpacity(0.8)');
        expect(output).toContain('await this.tac.__native.contentSurface.open(');
        expect(output).toContain('await this.tac.__native.screenCapture.listWindows()');
        expect(output).toContain("await this.tac.__native.host.invoke('example.echo'");
        expect(output).toContain("this.tac.__native.capabilities.supports('app.info')");
        expect(output).not.toContain('await app.info()');
    });

    test('lowers JavaScript providers only in executable code', () => {
        const source = String.raw`
export default class {
    async inspect() {
        const single = 'shortcuts.register'
        const double = "contentSurface.open"
        const template = ` + "`provider screenCapture.listWindows ${shortcuts.list()}`" + `
        const pattern = /contentSurface\\.open|shortcuts\\.register/
        // shortcuts.register and contentSurface.open are documentation here
        /* screenCapture.listWindows remains diagnostic text */
        await shortcuts.register({ id: 'toggle', accelerator: 'Primary+T' })
        return { single, double, template, pattern }
    }
}
`;

        const output = Compiler.lowerJavaScriptNativeShims(source);

        expect(output).toContain("const single = 'shortcuts.register'");
        expect(output).toContain('const double = "contentSurface.open"');
        expect(output).toContain('`provider screenCapture.listWindows ${this.tac.__native.shortcuts.list()}`');
        expect(output).toContain('/contentSurface\\.open|shortcuts\\.register/');
        expect(output).toContain('// shortcuts.register and contentSurface.open are documentation here');
        expect(output).toContain('/* screenCapture.listWindows remains diagnostic text */');
        expect(output).toContain('await this.tac.__native.shortcuts.register(');
    });

    test('rejects removed JavaScript wrapper names', () => {
        expect(() => Compiler.assertNoLegacyJavaScriptPlatformWrappers(
            'export default class { open() { return Browser.open("https://tachyon.del.ma") } }',
            '/app/client/pages/tac.js',
        )).toThrow('removed platform wrapper');
    });

    test('discovers each in-house language subset companion', async () => {
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-provider-'));
        tempDirs.push(root);
        await mkdir(path.join(root, 'client', 'components', 'raw'), { recursive: true });
        const templatePath = path.join(root, 'client', 'components', 'raw', 'tac.html');
        await writeFile(templatePath, '<span>{label}</span>');
        for (const [extension, language] of [
            ['.rs', 'rust'],
            ['.kt', 'kotlin'],
            ['.swift', 'swift'],
            ['.cs', 'csharp'],
        ]) {
            await writeFile(path.join(root, 'client', 'components', 'raw', `tac${extension}`), 'placeholder');
            const companion = await Compiler.getCompanionScript(templatePath);
            expect(companion?.provider).toEqual({ extension, language, target: 'subset', portable: true });
            await rm(path.join(root, 'client', 'components', 'raw', `tac${extension}`));
        }
    });
});
