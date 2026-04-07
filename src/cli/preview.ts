#!/usr/bin/env bun
import { access } from 'node:fs/promises'
import path from 'node:path'
import { createStaticPreviewServer } from '../runtime/static-preview.js'
import '../server/console-logger.js'

const distPath = path.join(process.cwd(), 'dist')

try {
    await access(distPath)
} catch {
    console.error(`Missing dist directory at '${distPath}'. Run 'tach.bundle' first.`, process.pid)
    process.exit(1)
}

const start = Date.now()
const server = await createStaticPreviewServer(distPath)

console.info(`Preview running on http://${server.hostname}:${server.port} — started in ${Date.now() - start}ms`, process.pid)

for (const signal of ['SIGINT', 'SIGTERM'] as const) {
    process.on(signal, () => {
        server.stop()
        process.exit(0)
    })
}
