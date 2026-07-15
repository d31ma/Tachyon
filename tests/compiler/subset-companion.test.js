// @ts-check
import { afterEach, describe, expect, test } from 'bun:test';
import { mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import { pathToFileURL } from 'url';
import TacSubsetCompanionCompiler from '../../src/compiler/subset-companion.js';

/** @type {string[]} */
const temporaryRoots = [];

afterEach(async () => {
    await Promise.all(temporaryRoots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

/** @param {string} code */
async function loadController(code) {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-subset-companion-'));
    temporaryRoots.push(root);
    const modulePath = path.join(root, 'tac.js');
    await writeFile(modulePath, code);
    return import(`${pathToFileURL(modulePath).href}?t=${Date.now()}`);
}

const sources = {
    rust: `
struct Counter {
    #[publish("counter.changed")]
    clicks: i32,
}

impl Counter {
    fn new() -> Self { Self { clicks: 0 } }

    fn increment(&mut self) {
        self.clicks += 1;
    }
}
`,
    kotlin: `
class Counter : Tac() {
    @publish("counter.changed")
    var clicks: Int = 0

    fun increment() {
        clicks += 1
    }
}
`,
    swift: `
final class Counter: Tac {
    @publish("counter.changed")
    var clicks: Int = 0

    func increment() {
        self.clicks += 1
    }
}
`,
    csharp: `
public class Counter : Tac {
    [Publish("counter.changed")]
    public int clicks = 0;

    public void Increment() {
        this.clicks += 1;
    }
}
`,
};

describe('TacSubsetCompanionCompiler', () => {
    for (const [language, source] of Object.entries(sources)) {
        test(`lowers a ${language} Tac companion into the reactive controller ABI`, async () => {
            const compiler = new TacSubsetCompanionCompiler(/** @type {'rust' | 'kotlin' | 'swift' | 'csharp'} */ (language));
            const output = compiler.compile(source, `/app/client/components/counter/tac.${language === 'csharp' ? 'cs' : language === 'kotlin' ? 'kt' : language === 'swift' ? 'swift' : 'rs'}`);
            const module = await loadController(output.code);
            const published = [];
            const controller = new module.default({}, {
                publish(name, value) { published.push({ name, value }); },
                subscribe() { return () => {}; },
                onMount() {},
            });

            expect(controller.clicks).toBe(0);
            await controller[language === 'csharp' ? 'Increment' : 'increment']();
            expect(controller.clicks).toBe(1);
            expect(controller.__tc_signal_publish_fields__).toEqual([
                { field: 'clicks', name: 'counter.changed', options: { retain: true } },
            ]);
        });
    }

    test('maps subscribe and mount annotations to the Tac runtime bindings', async () => {
        const output = new TacSubsetCompanionCompiler('kotlin').compile(`
class Counter : Tac() {
    var clicks: Int = 0

    @subscribe("counter.reset")
    fun reset(value: Int) { clicks = value }

    @onMount
    fun ready() { clicks = 1 }
}
`, '/app/client/components/counter/tac.kt');
        const module = await loadController(output.code);
        /** @type {((value: number) => void) | undefined} */
        let reset;
        /** @type {(() => void) | undefined} */
        let mount;
        const controller = new module.default({}, {
            publish() {},
            subscribe(_name, listener) { reset = listener; return () => {}; },
            onMount(listener) { mount = listener; },
        });

        reset?.(8);
        expect(controller.clicks).toBe(8);
        mount?.();
        expect(controller.clicks).toBe(1);
    });

    test('lowers the implicit Rust platform prelude without a framework namespace', async () => {
        const output = new TacSubsetCompanionCompiler('rust').compile(`
struct Bridge {
    web_status: String,
    device_status: String,
}

impl Bridge {
    fn new() -> Self { Self { web_status: "idle", device_status: "idle" } }

    fn inspect_web(&mut self) {
        local_storage().set_item("bridge", "ready");
        self.web_status = local_storage().get_item("bridge", "missing");
    }

    async fn inspect_device(&mut self) {
        if !app().is_available() {
            self.device_status = "not authorized";
            return;
        }
        let info = await app().info();
        self.device_status = info.name;
    }
}
`, '/app/client/components/bridge/tac.rs');
        const module = await loadController(output.code);
        const local = new Map();
        const controller = new module.default({}, {
            __native: {
                app: { available: () => false, info: async () => ({ name: 'Tachyon' }) },
                web: { localStorage: {
                    getItem(key, fallback) { return local.has(key) ? local.get(key) : fallback; },
                    setItem(key, value) { local.set(key, value); },
                } },
            },
            publish() {}, subscribe() { return () => {}; }, onMount() {},
        });

        controller.inspect_web();
        await controller.inspect_device();
        expect(controller.web_status).toBe('ready');
        expect(controller.device_status).toBe('not authorized');
    });

    test('lowers direct language-shaped clipboard APIs without exposing Tac names in source', () => {
        const examples = [
            ['kotlin', `class Bridge : Tac() { fun copy() { clipboard.writeText("ready") } }`, '__native.clipboard.writeText("ready")'],
            ['swift', `final class Bridge: Tac { func copy() { clipboard.writeText("ready") } }`, '__native.clipboard.writeText("ready")'],
            ['csharp', `public class Bridge : Tac { public void Copy() { Clipboard.SetTextAsync("ready"); LocalStorage.SetItem("bridge", "ready"); } }`, '__native.clipboard.writeText("ready")'],
        ];
        for (const [language, source, expected] of examples) {
            const output = new TacSubsetCompanionCompiler(/** @type {'kotlin' | 'swift' | 'csharp'} */ (language)).compile(source, `/app/client/components/bridge/tac.${language}`).code;
            expect(output).toContain(expected);
            expect(output).not.toContain('Clipboard.SetTextAsync');
            if (language === 'csharp')
                expect(output).toContain('__native.web.localStorage.setItem("bridge", "ready")');
        }
    });

    test('lowers language-shaped raw host APIs into the private capability bridge', () => {
        const examples = [
            ['rust', `struct Bridge {} impl Bridge { async fn save(&mut self) { file_system().write_text("/tmp/ready.txt", "ready"); shell().exec("echo", ["ready"]); } }`, '__native.fileSystem.writeText("/tmp/ready.txt", "ready")'],
            ['kotlin', `class Bridge : Tac() { fun save() { fileSystem.writeText("/tmp/ready.txt", "ready"); shell.exec("echo", ["ready"]) } }`, '__native.fileSystem.writeText("/tmp/ready.txt", "ready")'],
            ['swift', `final class Bridge: Tac { func save() { fileSystem.writeText("/tmp/ready.txt", "ready"); shell.exec("echo", ["ready"]) } }`, '__native.fileSystem.writeText("/tmp/ready.txt", "ready")'],
            ['csharp', `public class Bridge : Tac { public void Save() { FileSystem.WriteTextAsync("/tmp/ready.txt", "ready"); Shell.ExecAsync("echo", new[] { "ready" }); } }`, '__native.fileSystem.writeText("/tmp/ready.txt", "ready")'],
        ];
        for (const [language, source, expected] of examples) {
            const output = new TacSubsetCompanionCompiler(/** @type {'rust' | 'kotlin' | 'swift' | 'csharp'} */ (language)).compile(source, `/app/client/components/bridge/tac.${language}`).code;
            expect(output).toContain(expected);
            expect(output).toContain('__native.shell.exec');
        }
    });

    test('lowers the extended filesystem family (stat, mkdir, remove) for every companion language', () => {
        const examples = [
            ['rust', `struct Bridge {} impl Bridge { async fn run(&mut self) { file_system().stat("/tmp/a"); file_system().mkdir("/tmp/b"); file_system().remove("/tmp/c"); } }`],
            ['kotlin', `class Bridge : Tac() { fun run() { fileSystem.stat("/tmp/a"); fileSystem.mkdir("/tmp/b"); fileSystem.remove("/tmp/c") } }`],
            ['swift', `final class Bridge: Tac { func run() { fileSystem.stat("/tmp/a"); fileSystem.mkdir("/tmp/b"); fileSystem.remove("/tmp/c") } }`],
            ['csharp', `public class Bridge : Tac { public void Run() { FileSystem.StatAsync("/tmp/a"); FileSystem.MkdirAsync("/tmp/b"); FileSystem.RemoveAsync("/tmp/c"); } }`],
        ];
        for (const [language, source] of examples) {
            const output = new TacSubsetCompanionCompiler(/** @type {'rust' | 'kotlin' | 'swift' | 'csharp'} */ (language)).compile(source, `/app/client/components/bridge/tac.${language}`).code;
            expect(output).toContain('__native.fileSystem.stat("/tmp/a")');
            expect(output).toContain('__native.fileSystem.mkdir("/tmp/b")');
            expect(output).toContain('__native.fileSystem.remove("/tmp/c")');
        }
    });

    test('lowers the language-shaped FYLO collection facade for every portable companion language', () => {
        const examples = [
            ['rust', `struct Store { status: String } impl Store { fn new() -> Self { Self { status: "idle" } } async fn refresh(&mut self) { self.status = await fylo().collection("notes").find({}); } }`, 'this.tac.__native.fylo.collection("notes").find({})'],
            ['kotlin', `class Store : Tac() {
                var status: String = "idle"
                fun refresh() { status = fylo.collection("notes").find({}) }
            }`, 'this.tac.__native.fylo.collection("notes").find({})'],
            ['swift', `final class Store: Tac {
                var status: String = "idle"
                func refresh() { self.status = fylo.collection("notes").find({}) }
            }`, 'this.tac.__native.fylo.collection("notes").find({})'],
            ['csharp', `public class Store : Tac {
                public string status = "idle";
                public void Refresh() { this.status = Fylo.Collection("notes").Find({}); }
            }`, 'this.tac.__native.fylo.collection("notes").find({})'],
        ];

        for (const [language, source, expected] of examples) {
            const output = new TacSubsetCompanionCompiler(/** @type {'rust' | 'kotlin' | 'swift' | 'csharp'} */ (language)).compile(source, `/app/client/components/store/tac.${language}`).code;
            expect(output).toContain(expected);
            expect(output).not.toMatch(/\bFylo(?:\.|::)/);
        }
    });

    test('keeps prelude casing native to each language', () => {
        const examples = [
            ['rust', `struct Bridge { status: String } impl Bridge { fn new() -> Self { Self { status: "idle" } } async fn inspect(&mut self) { self.status = await fetch("/status"); } }`, '__native.web.fetch("/status")'],
            ['kotlin', `class Bridge : Tac() { var status: String = "idle"; fun inspect() { status = fetch("/status") } }`, '__native.web.fetch("/status")'],
            ['swift', `final class Bridge: Tac { var status: String = "idle"; func inspect() { self.status = fetch("/status") } }`, '__native.web.fetch("/status")'],
            ['csharp', `public class Bridge : Tac { public string status = "idle"; public void Inspect() { this.status = FetchAsync("/status"); } }`, '__native.web.fetch("/status")'],
        ];
        for (const [language, source, expected] of examples) {
            const output = new TacSubsetCompanionCompiler(/** @type {'rust' | 'kotlin' | 'swift' | 'csharp'} */ (language)).compile(source, `/app/client/components/bridge/tac.${language}`).code;
            expect(output).toContain(expected);
            expect(output).not.toContain('Web.');
            expect(output).not.toContain('Web::');
        }
    });

    test('lowers the shared mobile capabilities for every companion language', () => {
        const examples = [
            ['rust', `struct Bridge { status: String } impl Bridge { fn new() -> Self { Self { status: "idle" } } async fn notify(&mut self) { await share().text("ready"); await haptics().impact(); } }`, '__native.share.text("ready")'],
            ['kotlin', `class Bridge : Tac() { fun notify() { share.text("ready"); haptics.impact() } }`, '__native.share.text("ready")'],
            ['swift', `final class Bridge: Tac { func notify() { share.text("ready"); haptics.impact() } }`, '__native.share.text("ready")'],
            ['csharp', `public class Bridge : Tac { public void Notify() { Share.TextAsync("ready"); Haptics.ImpactAsync(); } }`, '__native.share.text("ready")'],
        ];
        for (const [language, source, expected] of examples) {
            const output = new TacSubsetCompanionCompiler(/** @type {'rust' | 'kotlin' | 'swift' | 'csharp'} */ (language)).compile(source, `/app/client/components/bridge/tac.${language}`).code;
            expect(output).toContain(expected);
            expect(output).toContain('__native.haptics.impact()');
        }
    });

    test('never rewrites prelude-shaped text inside string literals or comments', async () => {
        const output = new TacSubsetCompanionCompiler('kotlin').compile(`
class Bridge : Tac() {
    var message: String = "idle"

    fun probe() {
        // fetch("/status") in this comment must survive the lowering too
        message = "call fetch(later) and navigator.isOnline() by hand"
    }
}
`, '/app/client/components/bridge/tac.kt');
        expect(output.code).toContain('"call fetch(later) and navigator.isOnline() by hand"');
        expect(output.code).toContain('// fetch("/status") in this comment must survive the lowering too');
        expect(output.code).not.toContain('__native.web.fetch(later)');

        const module = await loadController(output.code);
        const controller = new module.default({}, { publish() {}, subscribe() { return () => {}; }, onMount() {} });
        controller.probe();
        expect(controller.message).toBe('call fetch(later) and navigator.isOnline() by hand');
    });

    test('a string mentioning a removed wrapper or an import keyword is not an error', () => {
        const output = new TacSubsetCompanionCompiler('kotlin').compile(`
class Bridge : Tac() {
    var message: String = "idle"

    fun explain() {
        message = "Web. wrappers and import statements are not allowed"
    }
}
`, '/app/client/components/bridge/tac.kt');
        expect(output.contract.className).toBe('Bridge');
    });

    test('rejects out-of-subset constructs by name with a line and column', () => {
        const examples = [
            ['kotlin', `class Bridge : Tac() {\n    var status: String = "idle"\n    fun pick(value: Int) {\n        when (value) { else -> {} }\n    }\n}`, "'when'", ':4:'],
            ['rust', `struct Bridge { status: String }\nimpl Bridge {\n    fn new() -> Self { Self { status: "idle" } }\n    fn pick(&mut self) {\n        match self.status { _ => {} }\n    }\n}`, "'match'", ':5:'],
            ['swift', `final class Bridge: Tac {\n    var status: String = "idle"\n    func pick() {\n        guard true else { return }\n    }\n}`, "'guard'", ':4:'],
            ['csharp', `public class Bridge : Tac {\n    public string status = "idle";\n    public void Pick() {\n        foreach (var item in this.status) {}\n    }\n}`, "'foreach'", ':4:'],
        ];
        for (const [language, source, construct, position] of examples) {
            let message = '';
            try {
                new TacSubsetCompanionCompiler(/** @type {'rust' | 'kotlin' | 'swift' | 'csharp'} */ (language)).compile(source, `/app/client/components/bridge/tac.${language}`);
            }
            catch (error) {
                message = error instanceof Error ? error.message : String(error);
            }
            expect(message).toContain(construct);
            expect(message).toContain('is not part of the Tac');
            expect(message).toContain(position);
        }
    });

    test('an out-of-subset keyword inside a string literal is allowed', () => {
        const output = new TacSubsetCompanionCompiler('kotlin').compile(`
class Bridge : Tac() {
    var status: String = "idle"

    fun explain() {
        status = "use while instead of for or when"
    }
}
`, '/app/client/components/bridge/tac.kt');
        expect(output.contract.className).toBe('Bridge');
    });

    test('rejects imports because Tac owns the portable companion runtime', () => {
        expect(() => new TacSubsetCompanionCompiler('swift').compile(`
import Foundation
class Counter: Tac {}
`, '/app/client/components/counter/tac.swift')).toThrow('must not declare imports');
    });

    test('rejects removed wrapper names instead of leaving unresolved runtime symbols', () => {
        expect(() => new TacSubsetCompanionCompiler('rust').compile(`
struct Counter { value: String }
impl Counter {
    fn new() -> Self { Self { value: "idle" } }
    fn ready(&mut self) { Web::local_storage_set("value", "ready"); }
}
`, '/app/client/components/counter/tac.rs')).toThrow('removed platform wrapper');
    });
});
