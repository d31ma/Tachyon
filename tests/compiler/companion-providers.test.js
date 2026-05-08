// @ts-check
import { afterEach, describe, expect, test } from 'bun:test';
import { chmod, mkdir, mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import Compiler from '../../src/compiler/index.js';

/** @type {string[]} */
const tempDirs = [];

afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

describe('Compiler companion providers', () => {
    test('discovers companions through provider metadata', async () => {
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-provider-'));
        tempDirs.push(root);
        await mkdir(path.join(root, 'components'), { recursive: true });
        const templatePath = path.join(root, 'components', 'card.html');
        const jsCompanionPath = path.join(root, 'components', 'card.js');
        const tsCompanionPath = path.join(root, 'components', 'card.ts');
        await writeFile(templatePath, '<article>{title}</article>');
        await writeFile(jsCompanionPath, 'export default class {}');
        await writeFile(tsCompanionPath, 'export default class {}');

        const companion = await Compiler.getCompanionScript(templatePath);

        expect(companion?.sourcePath).toBe(jsCompanionPath);
        expect(companion?.importPath).toBe('./card.js');
        expect(companion?.provider).toEqual({ extension: '.js', target: 'ecmascript' });
        expect(await Compiler.getCompanionScriptPath(templatePath)).toBe(jsCompanionPath);

        await rm(jsCompanionPath, { force: true });
        await rm(tsCompanionPath, { force: true });
        const wasmCompanionPath = path.join(root, 'components', 'card.wasm');
        await writeFile(wasmCompanionPath, new Uint8Array());
        const wasmCompanion = await Compiler.getCompanionScript(templatePath);
        expect(wasmCompanion?.sourcePath).toBe(wasmCompanionPath);
        expect(wasmCompanion?.provider).toEqual({ extension: '.wasm', target: 'wasm-json' });
    });

    test('prefers source-backed Wasm companions before prebuilt Wasm fallbacks', async () => {
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-provider-'));
        tempDirs.push(root);
        await mkdir(path.join(root, 'components'), { recursive: true });
        const templatePath = path.join(root, 'components', 'meter.html');
        const rustCompanionPath = path.join(root, 'components', 'meter.rs');
        const wasmCompanionPath = path.join(root, 'components', 'meter.wasm');
        await writeFile(templatePath, '<article>{label}</article>');
        await writeFile(rustCompanionPath, '/* rust source */');
        await writeFile(wasmCompanionPath, new Uint8Array());

        const companion = await Compiler.getCompanionScript(templatePath);

        expect(companion?.sourcePath).toBe(rustCompanionPath);
        expect(companion?.importPath).toBe('./meter.rs');
        expect(companion?.provider).toEqual({ extension: '.rs', target: 'wasm-source', language: 'rust' });
    });

    test('source-backed Wasm companions compile through the configured language tool', async () => {
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-provider-'));
        tempDirs.push(root);
        await mkdir(path.join(root, 'components'), { recursive: true });
        const templatePath = path.join(root, 'components', 'chip.html');
        const watCompanionPath = path.join(root, 'components', 'chip.wat');
        const fakeCompilerPath = path.join(root, 'fake-wat2wasm');
        await writeFile(templatePath, '<span>{label}</span>');
        await writeFile(watCompanionPath, '(module)');
        await writeFile(fakeCompilerPath, `#!/bin/sh
out=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-o" ]; then
    shift
    out="$1"
  fi
  shift
done
printf 'compiled-wasm' > "$out"
`);
        await chmod(fakeCompilerPath, 0o755);

        const originalCompilerPath = process.env.TACHYON_WASM_WAT2WASM;
        process.env.TACHYON_WASM_WAT2WASM = fakeCompilerPath;
        try {
            const companion = await Compiler.getCompanionScript(templatePath);
            expect(companion?.provider).toEqual({ extension: '.wat', target: 'wasm-source', language: 'wat' });
            const bytes = await Compiler.compileWasmSourceCompanion(/** @type {NonNullable<typeof companion>} */ (companion));
            expect(Buffer.from(bytes).toString('utf8')).toBe('compiled-wasm');
        }
        finally {
            if (originalCompilerPath === undefined)
                delete process.env.TACHYON_WASM_WAT2WASM;
            else
                process.env.TACHYON_WASM_WAT2WASM = originalCompilerPath;
        }
    });

    test('source-backed Wasm companions can use a sibling prebuilt fallback', async () => {
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-provider-'));
        tempDirs.push(root);
        await mkdir(path.join(root, 'components'), { recursive: true });
        const templatePath = path.join(root, 'components', 'fallback.html');
        const watCompanionPath = path.join(root, 'components', 'fallback.wat');
        const wasmFallbackPath = path.join(root, 'components', 'fallback.wasm');
        await writeFile(templatePath, '<span>{label}</span>');
        await writeFile(watCompanionPath, '(module)');
        await writeFile(wasmFallbackPath, 'prebuilt-fallback');

        const originalCompilerPath = process.env.TACHYON_WASM_WAT2WASM;
        process.env.TACHYON_WASM_WAT2WASM = path.join(root, 'missing-wat2wasm');
        try {
            const companion = await Compiler.getCompanionScript(templatePath);
            expect(companion?.provider).toEqual({ extension: '.wat', target: 'wasm-source', language: 'wat' });
            const bytes = await Compiler.getTacWasmBytes(/** @type {NonNullable<typeof companion>} */ (companion));
            expect(Buffer.from(bytes).toString('utf8')).toBe('prebuilt-fallback');
        }
        finally {
            if (originalCompilerPath === undefined)
                delete process.env.TACHYON_WASM_WAT2WASM;
            else
                process.env.TACHYON_WASM_WAT2WASM = originalCompilerPath;
        }
    });

    test('returns null when no provider matches a companion file', async () => {
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-provider-'));
        tempDirs.push(root);
        await mkdir(path.join(root, 'components'), { recursive: true });
        const templatePath = path.join(root, 'components', 'panel.html');
        await writeFile(templatePath, '<section>Panel</section>');

        expect(await Compiler.getCompanionScript(templatePath)).toBeNull();
        expect(await Compiler.getCompanionScriptPath(templatePath)).toBeNull();
    });
});
