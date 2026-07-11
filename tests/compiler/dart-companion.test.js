// @ts-check
import { afterEach, describe, expect, test } from 'bun:test';
import { mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import { pathToFileURL } from 'url';
import DartCompanionCompiler, { DartCompanionContract } from '../../src/compiler/dart-companion.js';
import DartToolchain, { DART_VERSION } from '../../src/compiler/dart-toolchain.js';

/** @type {string[]} */
const temporaryRoots = [];

afterEach(async () => {
    await Promise.all(temporaryRoots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe('DartCompanionContract', () => {
    test('uses a versioned Tachyon cache location for the managed Dart SDK', () => {
        const toolchain = new DartToolchain('/tmp/tachyon-cache');
        expect(toolchain.root()).toContain(path.join('toolchains', 'dart', DART_VERSION));
        expect(toolchain.commandPath()).toContain(path.join('dart-sdk', 'bin'));
        expect(toolchain.downloadUrl()).toContain(`/release/${DART_VERSION}/sdk/dartsdk-`);
        expect(toolchain.release().sha256).toMatch(/^[a-f0-9]{64}$/);
    });

    test('derives a Tac controller bridge from public Dart fields and methods', () => {
        const contract = DartCompanionContract.parse(`
class Counter extends Tac {
    @publish('counter.changed')
    int count = 0;

    @onMount()
    void ready() {
        count = 1;
    }

    @subscribe('counter.reset')
    void reset(dynamic value) {
        count = value as int;
    }

    void increment() {
        count += 1;
    }
}
`, '/app/client/components/counter/tac.dart');

        expect(contract.className).toBe('Counter');
        expect(contract.fields.map((field) => field.name)).toEqual(['count']);
        expect(contract.methods.map((method) => method.name)).toEqual(['ready', 'reset', 'increment']);
        expect(contract.publishedFields).toEqual([{ field: 'count', name: 'counter.changed' }]);
        expect(contract.mountMethods).toEqual(['ready']);
        expect(contract.subscriptions).toEqual([{ method: 'reset', name: 'counter.reset' }]);
    });

    test('rejects imports and constructors because Tac supplies the companion runtime', () => {
        expect(() => DartCompanionContract.parse(`
import 'package:outside/outside.dart';
class Counter extends Tac {}
`, '/app/client/components/counter/tac.dart')).toThrow('must not declare imports');

        expect(() => DartCompanionContract.parse(`
class Counter extends Tac {
    Counter(String label);
}
`, '/app/client/components/counter/tac.dart')).toThrow('must use the implicit constructor');

        expect(() => DartCompanionContract.parse(`
class Counter extends Tac {
    Future<void> save() async { await Web.localStorage.setItem('theme', 'calm'); }
}
`, '/app/client/components/counter/tac.dart')).toThrow('removed platform wrapper');
    });

    test('injects an import-free Dart platform prelude', () => {
        const compiler = new DartCompanionCompiler('dart');
        const source = 'class Counter extends Tac { int count = 0; }';
        const contract = DartCompanionContract.parse(source, '/app/client/components/counter/tac.dart');
        const generated = compiler.createDartSource(source, contract, '__tcDartTest');

        expect(generated).toContain('class _Clipboard');
        expect(generated).toContain('class _FileSystem');
        expect(generated).toContain('class _Shell');
        expect(generated).toContain('const clipboard = _Clipboard()');
        expect(generated).toContain('const fileSystem = _FileSystem()');
        expect(generated).toContain('const shell = _Shell()');
        expect(generated).toContain("'clipboard.writeText'");
        expect(generated).toContain("const localStorage = _WebStorage('localStorage')");
        expect(generated).toContain('const fylo = _Fylo()');
        expect(generated).toContain('const app = _App()');
        expect(generated).toContain('const share = _Share()');
        expect(generated).toContain('const haptics = _Haptics()');
        expect(generated).toContain('Future<dynamic> fetch');
    });

    test('compiles a Dart controller to the normal Tac companion module shape when Dart is available', async () => {
        if (!Bun.which('dart'))
            return;
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-dart-companion-'));
        temporaryRoots.push(root);
        const sourcePath = path.join(root, 'tac.dart');
        await writeFile(sourcePath, `
class Counter extends Tac {
    @publish('counter.changed')
    int count = 0;

    @onMount()
    void ready() {
        count = 1;
    }

    @subscribe('counter.reset')
    void reset(dynamic value) {
        count = value as int;
    }

    void increment() {
        count += 1;
    }
}
`);

        const output = await new DartCompanionCompiler().compile(sourcePath);
        const modulePath = path.join(root, 'tac.dart.js');
        await writeFile(modulePath, output.code);
        const module = await import(`${pathToFileURL(modulePath).href}?t=${Date.now()}`);
        const controller = new module.default({}, {
            subscribe() { return () => {}; },
            onMount() {},
        });

        expect(controller.count).toBe(0);
        expect(controller.__tc_signal_publish_fields__).toEqual([
            { field: 'count', name: 'counter.changed', options: { retain: true } },
        ]);
        await controller.increment();
        expect(controller.count).toBe(1);
    });

    test('routes the Dart implicit platform prelude through the private capability bridge', async () => {
        if (!Bun.which('dart'))
            return;
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-dart-native-shim-'));
        temporaryRoots.push(root);
        const sourcePath = path.join(root, 'tac.dart');
        await writeFile(sourcePath, `
class Counter extends Tac {
    Future<void> save() async {
        await clipboard.writeText('Tac');
        await localStorage.setItem('theme', 'calm');
    }
}
`);

        const output = await new DartCompanionCompiler().compile(sourcePath);
        const modulePath = path.join(root, 'tac.dart.js');
        await writeFile(modulePath, output.code);
        const module = await import(`${pathToFileURL(modulePath).href}?t=${Date.now()}`);
        const calls = [];
        const controller = new module.default({}, {
            __nativeCallback(operation, payload, resolve) {
                const decoded = payload ? JSON.parse(payload) : undefined;
                calls.push({ operation, payload: decoded });
                resolve(undefined);
            },
            __nativeAvailable() {
                return true;
            },
        });

        await controller.save();
        expect(calls).toEqual([
            { operation: 'clipboard.writeText', payload: { text: 'Tac' } },
            { operation: 'web.localStorage.setItem', payload: { key: 'theme', value: 'calm' } },
        ]);
    });

    test('routes the Dart FYLO prelude through the browser engine bridge', async () => {
        if (!Bun.which('dart'))
            return;
        const root = await mkdtemp(path.join(tmpdir(), 'tachyon-dart-fylo-shim-'));
        temporaryRoots.push(root);
        const sourcePath = path.join(root, 'tac.dart');
        await writeFile(sourcePath, `
class Counter extends Tac {
    Future<void> refresh() async {
        await fylo.collection('notes').find({ 'limit': 1 });
    }
}
`);

        const output = await new DartCompanionCompiler().compile(sourcePath);
        const modulePath = path.join(root, 'tac.dart.js');
        await writeFile(modulePath, output.code);
        const module = await import(`${pathToFileURL(modulePath).href}?t=${Date.now()}`);
        const calls = [];
        const controller = new module.default({}, {
            __nativeCallback(operation, payload, resolve) {
                calls.push({ operation, payload: payload ? JSON.parse(payload) : undefined });
                resolve({ docs: [] });
            },
        });

        await controller.refresh();
        expect(calls).toEqual([
            { operation: 'fylo.collection.find', payload: { collection: 'notes', args: [{ limit: 1 }] } },
        ]);
    });
});
