// @ts-check
// Build-speed harness. Generates synthetic Tac projects of increasing size, then
// measures cold full-build time (with a compile/write/prerender split) and a
// single-page incremental recompile for each. Answers the only question that
// decides whether a Rust port would help: where does the build time actually go?
//
//   bun scripts/bench/run.mjs                 # default sizes
//   bun scripts/bench/run.mjs --sizes 50/10,500/100,1000/200
//
// Each size is "<pages>/<components>". Projects are written under the OS temp dir
// and removed after each run.
import { mkdir, writeFile, rm } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';

const repoRoot = path.resolve(import.meta.dir, '../..');
const measureScript = path.join(repoRoot, 'scripts/bench/measure.mjs');

/** @param {string} arg @param {string} fallback */
const flag = (arg, fallback) => {
    const i = process.argv.indexOf(arg);
    return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
};

const sizes = flag('--sizes', '50/10,200/40,500/100')
    .split(',')
    .map((s) => s.trim().split('/').map(Number))
    .map(([pages, components]) => ({ pages, components: components || Math.max(3, Math.round(pages / 5)) }));

// ── synthetic project generator ────────────────────────────────────────────
// Pages and components exercise the compiler's real hot paths: interpolation,
// loops, logic branches, event handlers, and component references.

/** @param {number} i */
const componentHtml = (i) => `<section class="c${i}">
  <h2>{title}</h2>
  <button on:click="bump($event)">count {count}</button>
  <ul>
    <loop :for="item of items">
      <li>{item.label}: {item.value}</li>
    </loop>
  </ul>
  <logic :if="count > 0"><p>positive</p></logic>
  <logic :if="count == 0"><p>zero</p></logic>
</section>
`;

/** @param {number} i */
const componentJs = (i) => `// @ts-check
export default class {
  title = 'Card ${i}'
  count = 0
  items = [{ label: 'a', value: 1 }, { label: 'b', value: 2 }, { label: 'c', value: 3 }]
  bump() { this.count++ }
}
`;

/** @param {number} i @param {number} componentCount */
const pageHtml = (i, componentCount) => {
    const refs = [0, 1, 2].map((k) => `  <gen-c${(i + k) % componentCount} />`).join('\n');
    return `<main>
  <h1>{heading}</h1>
  <p>{description}</p>
${refs}
  <loop :for="row of rows">
    <article><h3>{row.name}</h3><span>{row.score}</span></article>
  </loop>
  <logic :if="ready"><em>ready</em></logic>
  <logic :if="!ready"><em>loading</em></logic>
</main>
`;
};

/** @param {number} i */
const pageJs = (i) => `// @ts-check
export default class {
  heading = 'Page ${i}'
  description = 'Synthetic benchmark page ${i}'
  ready = true
  rows = Array.from({ length: 8 }, (_, n) => ({ name: 'row ' + n, score: n * ${i + 1} }))
}
`;

/** @param {number} pages @param {number} components */
async function generateProject(pages, components) {
    const root = path.join(tmpdir(), `tac-bench-${pages}p-${components}c-${Date.now()}`);
    await mkdir(root, { recursive: true });
    await writeFile(path.join(root, 'package.json'), JSON.stringify({ name: 'tac-bench', version: '1.0.0' }, null, 2));

    const componentsDir = path.join(root, 'client/components/gen');
    await Promise.all(Array.from({ length: components }, async (_, i) => {
        const dir = path.join(componentsDir, `c${i}`);
        await mkdir(dir, { recursive: true });
        await writeFile(path.join(dir, 'tac.html'), componentHtml(i));
        await writeFile(path.join(dir, 'tac.js'), componentJs(i));
    }));

    const pagesDir = path.join(root, 'client/pages');
    await mkdir(pagesDir, { recursive: true });
    await writeFile(path.join(pagesDir, 'tac.html'), `<main><h1>Home</h1></main>\n`);
    await writeFile(path.join(pagesDir, 'tac.js'), pageJs(0));
    await Promise.all(Array.from({ length: pages }, async (_, i) => {
        const dir = path.join(pagesDir, `p${i}`);
        await mkdir(dir, { recursive: true });
        await writeFile(path.join(dir, 'tac.html'), pageHtml(i, components));
        await writeFile(path.join(dir, 'tac.js'), pageJs(i));
    }));

    return { root, files: pages * 2 + components * 2 + 3 };
}

// ── run one size ───────────────────────────────────────────────────────────
/** @param {{ pages: number, components: number }} size */
async function measure({ pages, components }) {
    const { root, files } = await generateProject(pages, components);
    try {
        const proc = Bun.spawn(['bun', measureScript], {
            cwd: root,
            env: {
                ...process.env,
                TAC_BENCH: '1',
                TAC_BUNDLE_TARGET: 'web',
                NODE_ENV: 'development',
                // Enabled unless --no-cache: measures the persistent prerender cache.
                // Ids are deterministic by default now, so no id flag is needed.
                // --changed N sets how many pages a "partial" rebuild edits (default 5).
                ...(process.argv.includes('--no-cache')
                    ? {}
                    : { TAC_PRERENDER_CACHE: '1', TAC_BENCH_CHANGED: flag('--changed', '5') }),
            },
            stdout: 'pipe',
            stderr: 'inherit',
        });
        const out = await new Response(proc.stdout).text();
        await proc.exited;
        const line = out.split('\n').find((l) => l.startsWith('__BENCH__'));
        if (!line)
            throw new Error(`no bench output (exit ${proc.exitCode})\n${out.slice(-2000)}`);
        return { pages, components, files, ...JSON.parse(line.slice('__BENCH__'.length)) };
    }
    finally {
        await rm(root, { recursive: true, force: true });
    }
}

// ── report ─────────────────────────────────────────────────────────────────
const ms = (n) => (n == null ? '   n/a' : `${Math.round(n)}`.padStart(6));
const pct = (part, whole) => (whole ? `${Math.round((part / whole) * 100)}%`.padStart(4) : '   -');

const results = [];
for (const size of sizes) {
    process.stderr.write(`building ${size.pages} pages / ${size.components} components…\n`);
    results.push(await measure(size));
}

const cached = results.some((r) => r.warmMs != null);
const hitStr = (c) => (c ? `${c.hits}/${c.hits + c.misses}` : '-');
console.log('\nBuild-speed benchmark (cold = full build; warm/partial = rebuild w/ prerender cache)\n');
const header = cached
    ? 'pages  comps │  cold(ms) │  warm(ms)  hits │  partial(ms)  hits  (Δpages) │  incr(ms)'
    : 'pages  comps │  cold(ms)  compile        │  write  prerender │  incr(ms)';
console.log(header);
console.log('─'.repeat(header.length));
for (const r of results) {
    if (cached) {
        console.log(
            `${String(r.pages).padStart(5)}  ${String(r.components).padStart(5)} │ ` +
            `${ms(r.coldMs)} │ ${ms(r.warmMs)}  ${hitStr(r.cache).padStart(9)} │ ` +
            `${ms(r.partialMs)}  ${hitStr(r.partialCache).padStart(9)}  ${`(${r.changed})`.padStart(6)} │ ${ms(r.incrementalMs)}`
        );
    }
    else {
        const c = r.split.compile ?? 0;
        console.log(
            `${String(r.pages).padStart(5)}  ${String(r.components).padStart(5)} │ ` +
            `${ms(r.coldMs)}   ${ms(c)} ${pct(c, r.coldMs)} │ ${ms(r.split.write)} ${ms(r.split.prerender)}   │ ${ms(r.incrementalMs)}`
        );
    }
}
console.log(cached
    ? '\nwarm    = rebuild, source unchanged (cache ceiling — all routes hit).\n'
      + 'partial = rebuild after editing Δpages files (the realistic CI case — those miss, rest hit).\n'
      + 'hits    = routes served from cache / total. Once prerender is cached, the compile phase is the floor.\n'
    : '\ncompile = your template→JS pass (the only slice a Rust port could speed up).\n');
