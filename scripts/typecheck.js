#!/usr/bin/env bun
// @ts-check

/**
 * Semantic type-safety gate for Tachyon's release pipeline.
 *
 * Runs `tsc --noEmit` against tsconfig.src.json — the deterministic gate
 * required by the release policy. Other scoped projects (tsconfig.tests.json,
 * tsconfig.examples.json) can be passed as arguments; only src is the
 * release blocker.
 *
 * Performance note: tsc spends >99% of its wall-clock on `I/O Read time`
 * when the repo lives on a cloud-synced filesystem (Dropbox, iCloud, etc.).
 * In that environment a full src typecheck runs ~5 minutes the first time,
 * then drops once filesystem caches warm up. On a CI runner or a local
 * disk, the same check completes in seconds. We bound the run with a
 * generous watchdog so genuine hangs are still caught.
 *
 * Override the watchdog with `TACHYON_TYPECHECK_TIMEOUT_MS` if needed.
 */

import path from 'path';

const TIMEOUT_MS = Number(process.env.TACHYON_TYPECHECK_TIMEOUT_MS) || 600_000;
const projectRoot = path.resolve(import.meta.dir, '..');
const tscPath = path.join(projectRoot, 'node_modules', 'typescript', 'bin', 'tsc');
const configs = process.argv.slice(2);
if (configs.length === 0) configs.push('tsconfig.src.json');

for (const config of configs) {
    const label = path.basename(config);
    console.log(`Typecheck → ${label}`);
    const start = Date.now();
    const proc = Bun.spawn(['node', tscPath, '-p', config, '--noEmit', '--pretty', 'false'], {
        cwd: projectRoot,
        stdout: 'inherit',
        stderr: 'inherit',
    });
    /** @type {ReturnType<typeof setTimeout> | undefined} */
    let watchdog;
    const code = await Promise.race([
        proc.exited,
        new Promise((resolve) => {
            watchdog = setTimeout(() => {
                proc.kill();
                resolve(124);
            }, TIMEOUT_MS);
        }),
    ]);
    if (watchdog) clearTimeout(watchdog);
    const elapsed = ((Date.now() - start) / 1000).toFixed(1);
    if (code === 124) {
        throw new Error(`Typecheck timed out after ${TIMEOUT_MS}ms for ${label}`);
    }
    if (code !== 0) {
        throw new Error(`Typecheck failed for ${label} (exit ${code}, ${elapsed}s)`);
    }
    console.log(`Typecheck ✓ ${label} (${elapsed}s)`);
}
