#!/usr/bin/env bun
// @ts-check
import path from 'path';
import { createAppScaffold } from '../runtime/app-scaffold.js';
import logger from '../server/logger.js';
const initLogger = logger.child({ scope: 'cli:init' });
const targetArg = process.argv[2] || '.';
const targetDir = path.resolve(process.cwd(), targetArg);
try {
    const created = await createAppScaffold(targetDir);
    initLogger.info('Scaffolded Tachyon app', { path: created });
    initLogger.info('Next steps', { command: `cd ${targetArg} && bun install && bun run start` });
}
catch (error) {
    initLogger.error('Failed to scaffold app', { err: error, path: targetDir });
    process.exit(1);
}
