#!/usr/bin/env bun
// @ts-check

/**
 * Semantic type-safety gate for Tachyon's release pipeline.
 *
 * The release gate still runs real `tsc --noEmit`. On cloud-synced working
 * copies, TypeScript can spend minutes walking dependency metadata in place,
 * so the runner stages the minimal project into the OS temp directory and
 * runs the same compiler there. CI and normal local checkouts run directly.
 *
 * Controls:
 * - TACHYON_TYPECHECK_TIMEOUT_MS: watchdog per project.
 * - TACHYON_TYPECHECK_STAGE=1: force staging.
 * - TACHYON_TYPECHECK_STAGE=0: force direct-in-repo execution.
 * - TACHYON_TYPECHECK_KEEP_STAGE=1: keep the temp directory for debugging.
 */

import { cp, mkdtemp, rm, stat } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';

const TIMEOUT_MS = Number(process.env.TACHYON_TYPECHECK_TIMEOUT_MS) || 120_000;
const projectRoot = path.resolve(import.meta.dir, '..');
const requestedConfigs = process.argv.slice(2);
const configs = requestedConfigs.length > 0 ? requestedConfigs : ['tsconfig.src.json'];
const includesWebsite = configs.some(config => path.basename(config) === 'tsconfig.website.json');

/** @type {Record<string, string[]>} */
const CONFIG_ROOTS = {
    'tsconfig.src.json': ['src'],
    // Tests import a few showcase modules (e.g. the realtime repository), so the
    // staged tests project must include `website` for those imports to resolve.
    'tsconfig.tests.json': ['src', 'scripts', 'tests', 'website'],
    'tsconfig.website.json': ['src', 'website'],
};

/**
 * @param {string} filePath
 * @returns {Promise<boolean>}
 */
async function exists(filePath) {
    try {
        await stat(filePath);
        return true;
    } catch {
        return false;
    }
}

/**
 * @param {string} root
 * @returns {boolean}
 */
function isCloudSyncedPath(root) {
    return [
        `${path.sep}CloudStorage${path.sep}`,
        `${path.sep}Dropbox${path.sep}`,
        `${path.sep}iCloud Drive${path.sep}`,
        `${path.sep}OneDrive${path.sep}`,
        `${path.sep}Google Drive${path.sep}`,
    ].some(marker => root.includes(marker));
}

/**
 * @returns {boolean}
 */
function shouldStageTypecheck() {
    if (process.env.TACHYON_TYPECHECK_STAGE === '1')
        return true;
    if (process.env.TACHYON_TYPECHECK_STAGE === '0')
        return false;
    if (process.env.CI)
        return false;
    return isCloudSyncedPath(projectRoot);
}

/**
 * @param {string} config
 * @returns {string[]}
 */
function rootsForConfig(config) {
    const basename = path.basename(config);
    return CONFIG_ROOTS[basename] ?? ['src', 'scripts', 'tests', 'website'];
}

/**
 * @param {string} config
 * @returns {string}
 */
function stagedConfigPath(config) {
    return path.basename(config);
}

/**
 * @param {string[]} selectedConfigs
 * @returns {Promise<string>}
 */
async function createTypecheckStage(selectedConfigs) {
    const stageRoot = await mkdtemp(path.join(tmpdir(), 'tachyon-typecheck-'));
    const roots = new Set(selectedConfigs.flatMap(rootsForConfig));
    const topLevelFiles = [
        'package.json',
        'bun.lock',
        'tsconfig.json',
        'tsconfig.base.json',
        'tsconfig.src.json',
        'tsconfig.tests.json',
        'tsconfig.website.json',
    ];

    for (const file of topLevelFiles) {
        const from = path.join(projectRoot, file);
        if (await exists(from))
            await cp(from, path.join(stageRoot, file));
    }

    for (const root of roots) {
        const from = path.join(projectRoot, root);
        if (!(await exists(from)))
            continue;
        await cp(from, path.join(stageRoot, root), {
            recursive: true,
            filter: (source) => {
                const basename = path.basename(source);
                return ![
                    '.git',
                    'coverage',
                    'dist',
                    'dist-debug',
                    'graphify-out',
                    'node_modules',
                ].includes(basename);
            },
        });
    }

    return stageRoot;
}

/**
 * @param {string} label
 * @param {string} cwd
 * @param {string[]} command
 * @returns {Promise<void>}
 */
async function runCommand(label, cwd, command) {
    const start = Date.now();
    const proc = Bun.spawn(command, {
        cwd,
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
    if (code === 124)
        throw new Error(`${label} timed out after ${TIMEOUT_MS}ms`);
    if (code !== 0)
        throw new Error(`${label} failed (exit ${code}, ${elapsed}s)`);
}

const stageTypecheck = shouldStageTypecheck();
/** @type {string | undefined} */
let typecheckRoot;

try {
    typecheckRoot = stageTypecheck ? await createTypecheckStage(configs) : projectRoot;
    if (stageTypecheck) {
        console.log(`Typecheck staging → ${typecheckRoot}`);
        await runCommand('Typecheck dependency install', typecheckRoot, [
            'bun',
            'install',
            '--frozen-lockfile',
            '--ignore-scripts',
        ]);
        if (includesWebsite && await exists(path.join(typecheckRoot, 'website', 'package.json'))) {
            await runCommand('Typecheck website dependency install', path.join(typecheckRoot, 'website'), [
                'bun',
                'install',
                '--frozen-lockfile',
                '--ignore-scripts',
            ]);
        }
    }

    const tscPath = path.join(typecheckRoot, 'node_modules', 'typescript', 'bin', 'tsc');
    for (const config of configs) {
        const label = path.basename(config);
        console.log(`Typecheck → ${label}${stageTypecheck ? ' (staged)' : ''}`);
        const start = Date.now();
        await runCommand(`Typecheck ${label}`, typecheckRoot, [
            'node',
            tscPath,
            '-p',
            stageTypecheck ? stagedConfigPath(config) : config,
            '--noEmit',
            '--pretty',
            'false',
        ]);
        const elapsed = ((Date.now() - start) / 1000).toFixed(1);
        console.log(`Typecheck ✓ ${label} (${elapsed}s)`);
    }
} finally {
    if (stageTypecheck && typecheckRoot && process.env.TACHYON_TYPECHECK_KEEP_STAGE !== '1')
        await rm(typecheckRoot, { recursive: true, force: true });
}
