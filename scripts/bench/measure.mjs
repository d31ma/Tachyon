// @ts-check
// Runs INSIDE a synthetic project (cwd = project root). Times a cold full build
// with a phase split and one incremental single-page recompile, then — when
// TAC_PRERENDER_CACHE is set — a second build with the cache warm to show how
// much of the prerender phase is cacheable. Prints one JSON line on stdout.
// Spawned by run.mjs.
import { runBuild } from '../../src/cli/bundle.js';
import Compiler from '../../src/compiler/index.js';
import { readFile, writeFile } from 'fs/promises';
import path from 'path';

const distWeb = path.join(process.cwd(), 'dist', 'web');

/** @param {() => Promise<void>} build */
async function timedBuild(build) {
    globalThis.__TAC_BENCH__ = [];
    const start = performance.now();
    await build();
    const totalMs = performance.now() - start;
    /** @type {Record<string, number>} */
    const split = {};
    for (const { phase, ms } of globalThis.__TAC_BENCH__)
        split[phase] = (split[phase] ?? 0) + ms;
    return { totalMs, split, cache: Compiler.prerenderCacheStats ?? null };
}

// Build 1: cold. If caching is on, this also populates the cache (all misses).
const cold = await timedBuild(runBuild);

// Incremental: edit-one-page cost — the core of the watch path (bundlePageFile +
// prerender one route). In-memory compiler state is warm from the build above.
let incrementalMs = null;
try {
    const pageRoute = `p1/${Compiler.pageFileName}`;
    const routePath = Compiler.routePathFromPageSource(pageRoute);
    const incStart = performance.now();
    await Compiler.bundlePageFile(pageRoute);
    await Compiler.prerenderRoutes(distWeb, [routePath]);
    incrementalMs = performance.now() - incStart;
}
catch (error) {
    process.stderr.write(`incremental probe failed: ${error?.message ?? error}\n`);
}

// Build 2: warm cache, unchanged source — the ceiling of what caching removes.
let warm = null;
// Build 3: change K pages, rebuild — the realistic CI case where a commit touches
// a few files. The K changed pages miss (re-render); the rest hit.
let partial = null;
const changed = Number(process.env.TAC_BENCH_CHANGED ?? 0);
if (process.env.TAC_PRERENDER_CACHE) {
    try {
        warm = await timedBuild(runBuild);
        if (changed > 0) {
            for (let i = 0; i < changed; i++) {
                const file = path.join(process.cwd(), `client/pages/p${i}/tac.js`);
                const src = await readFile(file, 'utf8').catch(() => null);
                if (src != null)
                    await writeFile(file, src.replace(/Synthetic benchmark page \d+/, `Edited page ${i} ${Date.now()}`));
            }
            partial = await timedBuild(runBuild);
        }
    }
    catch (error) {
        process.stderr.write(`warm/partial build failed: ${error?.message ?? error}\n`);
    }
}

process.stdout.write(`\n__BENCH__${JSON.stringify({
    coldMs: cold.totalMs,
    split: cold.split,
    incrementalMs,
    warmMs: warm?.totalMs ?? null,
    warmSplit: warm?.split ?? null,
    cache: warm?.cache ?? null,
    changed,
    partialMs: partial?.totalMs ?? null,
    partialSplit: partial?.split ?? null,
    partialCache: partial?.cache ?? null,
})}\n`);
