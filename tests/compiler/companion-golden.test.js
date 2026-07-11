// @ts-check
//
// Golden cross-language companion test: one controller spec written in every
// portable subset language must produce identical state transitions to a
// plain JavaScript reference. This is the test that catches "the dialect
// compiles but lies" failures — including the documented rule that arithmetic
// follows JavaScript semantics on every target (7 / 2 is 3.5, not 3).
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
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-companion-golden-'));
    temporaryRoots.push(root);
    const modulePath = path.join(root, 'tac.js');
    await writeFile(modulePath, code);
    return import(`${pathToFileURL(modulePath).href}?t=${Date.now()}`);
}

const goldenSources = {
    rust: `
struct Golden {
    total: i32,
    label: String,
}

impl Golden {
    fn new() -> Self { Self { total: 0, label: "idle" } }

    fn apply(&mut self) {
        self.total = (self.total + 7) / 2;
        local_storage().set_item("golden.label", "applied");
        self.label = local_storage().get_item("golden.label", "missing");
    }
}
`,
    kotlin: `
class Golden : Tac() {
    var total: Int = 0
    var label: String = "idle"

    fun apply() {
        total = (total + 7) / 2
        localStorage.setItem("golden.label", "applied")
        label = localStorage.getItem("golden.label", "missing")
    }
}
`,
    swift: `
final class Golden: Tac {
    var total: Int = 0
    var label: String = "idle"

    func apply() {
        self.total = (self.total + 7) / 2
        localStorage.setItem("golden.label", "applied")
        self.label = localStorage.getItem("golden.label", "missing")
    }
}
`,
    csharp: `
public class Golden : Tac {
    public int total = 0;
    public string label = "idle";

    public void apply() {
        this.total = (this.total + 7) / 2;
        LocalStorage.SetItem("golden.label", "applied");
        this.label = LocalStorage.GetItem("golden.label", "missing");
    }
}
`,
};

const extensions = { rust: 'rs', kotlin: 'kt', swift: 'swift', csharp: 'cs' };

function createTacBindings() {
    const stored = new Map();
    return {
        stored,
        tac: {
            __native: {
                web: {
                    localStorage: {
                        getItem(key, fallback) { return stored.has(key) ? stored.get(key) : fallback; },
                        setItem(key, value) { stored.set(key, value); },
                    },
                },
            },
            publish() { return false; },
            subscribe() { return () => {}; },
            onMount() {},
        },
    };
}

/** JavaScript reference the dialects must match exactly. */
function referenceRun() {
    const { stored, tac } = createTacBindings();
    let total = 0;
    total = (total + 7) / 2;
    tac.__native.web.localStorage.setItem('golden.label', 'applied');
    const label = tac.__native.web.localStorage.getItem('golden.label', 'missing');
    return { total, label, storedLabel: stored.get('golden.label') };
}

describe('portable companion golden behavior', () => {
    const expected = referenceRun();

    for (const [language, source] of Object.entries(goldenSources)) {
        test(`${language} matches the JavaScript reference (including JS numeric semantics)`, async () => {
            const output = new TacSubsetCompanionCompiler(/** @type {'rust' | 'kotlin' | 'swift' | 'csharp'} */ (language))
                .compile(source, `/app/client/components/golden/tac.${extensions[language]}`);
            const module = await loadController(output.code);
            const { stored, tac } = createTacBindings();
            const controller = new module.default({}, tac);

            expect(controller.total).toBe(0);
            expect(controller.label).toBe('idle');
            await controller.apply();
            expect(controller.total).toBe(expected.total);
            expect(controller.total).toBe(3.5);
            expect(controller.label).toBe(expected.label);
            expect(stored.get('golden.label')).toBe(expected.storedLabel);
        });
    }
});
