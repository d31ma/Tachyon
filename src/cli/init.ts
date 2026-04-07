#!/usr/bin/env bun
import path from 'node:path'
import { createAppScaffold } from '../runtime/app-scaffold.js'
import '../server/console-logger.js'

const targetArg = process.argv[2] || '.'
const targetDir = path.resolve(process.cwd(), targetArg)

try {
    const created = await createAppScaffold(targetDir)
    console.info(`Scaffolded Tachyon app in ${created}`, process.pid)
    console.info(`Next steps: cd ${targetArg} && bun install && bun run start`, process.pid)
} catch (error) {
    console.error((error as Error).message, process.pid)
    process.exit(1)
}
