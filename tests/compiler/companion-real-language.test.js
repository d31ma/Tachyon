// @ts-check
//
// Keeps the Kotlin Tac dialect a strict subset of real Kotlin: the showcase
// companion plus the shipped editor prelude must compile under `kotlinc`.
// Skipped when no Kotlin toolchain is installed.
//
// Kotlin is the only dialect this can verify: Rust and Swift companions use
// Tac annotations (#[onMount], @onMount) that their real compilers reject
// without a plugin, and C# companions use dialect forms such as `{}` query
// arguments. Those dialects are covered by the golden behavior tests instead.
import { afterEach, describe, expect, test } from 'bun:test';
import { mkdtemp, rm } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';

const kotlinc = Bun.which('kotlinc');
const projectRoot = path.resolve(import.meta.dir, '../..');

/** @type {string[]} */
const temporaryRoots = [];

afterEach(async () => {
    await Promise.all(temporaryRoots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe('portable companion real-language check', () => {
    test.skipIf(!kotlinc)('the Kotlin showcase companion compiles under real kotlinc with the editor prelude', async () => {
        const outputRoot = await mkdtemp(path.join(tmpdir(), 'tachyon-kotlinc-'));
        temporaryRoots.push(outputRoot);
        const compile = Bun.spawn({
            cmd: [
                /** @type {string} */ (kotlinc),
                path.join(projectRoot, 'src/runtime/companion-preludes/TacPrelude.kt'),
                path.join(projectRoot, 'website/client/components/language/kotlin/tac.kt'),
                '-d', outputRoot,
            ],
            stdout: 'pipe',
            stderr: 'pipe',
        });
        const [stderr, exitCode] = await Promise.all([
            new Response(compile.stderr).text(),
            compile.exited,
        ]);
        expect(`${exitCode}:${stderr}`.trim()).toBe('0:');
    }, 120_000);
});
