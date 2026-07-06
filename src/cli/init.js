#!/usr/bin/env bun
// @ts-check
import path from 'path';
import { createInterface } from 'readline/promises';
import { createAppScaffold } from '../runtime/app-scaffold.js';
import logger from '../server/observability/logger.js';
const initLogger = logger.child({ scope: 'cli:init' });

/**
 * @param {string[]} argv
 * @returns {{ targetArg: string, appName: string | null }}
 */
function parseArgs(argv) {
    let targetArg = '.';
    let appName = process.env.TAC_APP_NAME || null;
    for (let index = 2; index < argv.length; index += 1) {
        const arg = argv[index];
        if (arg === '--name' || arg === '--app-name') {
            appName = argv[index + 1] ?? '';
            index += 1;
            continue;
        }
        if (arg.startsWith('--name=')) {
            appName = arg.slice('--name='.length);
            continue;
        }
        if (arg.startsWith('--app-name=')) {
            appName = arg.slice('--app-name='.length);
            continue;
        }
        if (!arg.startsWith('-') && targetArg === '.') {
            targetArg = arg;
        }
    }
    return { targetArg, appName };
}

/** @param {string} targetDir */
function defaultAppName(targetDir) {
    const base = path.basename(path.resolve(process.cwd(), targetDir));
    return base && base !== '.' ? base : 'Tachyon App';
}

/**
 * @param {string} targetDir
 * @param {string | null} providedName
 */
async function resolveAppName(targetDir, providedName) {
    if (providedName && providedName.trim())
        return providedName.trim();
    if (!process.stdin.isTTY || !process.stdout.isTTY)
        return defaultAppName(targetDir);
    const rl = createInterface({ input: process.stdin, output: process.stdout });
    try {
        const fallback = defaultAppName(targetDir);
        const answer = await rl.question(`App name (${fallback}): `);
        return answer.trim() || fallback;
    }
    finally {
        rl.close();
    }
}

const { targetArg, appName: providedAppName } = parseArgs(process.argv);
const targetDir = path.resolve(process.cwd(), targetArg);
try {
    const appName = await resolveAppName(targetArg, providedAppName);
    const created = await createAppScaffold(targetDir, { appName });
    initLogger.info('Scaffolded Tachyon app', { path: created, appName });
    initLogger.info('Next steps', { command: `cd ${targetArg} && bun install && bun run start` });
}
catch (error) {
    initLogger.error('Failed to scaffold app', { err: error, path: targetDir });
    process.exit(1);
}
