#!/usr/bin/env bun
import { access, readdir, stat } from 'node:fs/promises'
import path from 'node:path'
import Router from '../server/route-handler.js'
import { createStaticPreviewServer } from '../runtime/static-preview.js'
import '../server/console-logger.js'

const distPath = path.join(process.cwd(), 'dist')
const watchMode = process.argv.includes('--watch') || process.argv.includes('--bundle-watch')
const WATCH_INTERVAL_MS = 300
const bundleCliPath = path.join(import.meta.dir, 'bundle.ts')

type PreviewWatcherHandle = {
    close(): void
}

async function pathFingerprint(targetPath: string): Promise<string[]> {
    try {
        const info = await stat(targetPath)
        if (info.isFile()) {
            return [`${targetPath}:${info.size}:${info.mtimeMs}`]
        }

        if (!info.isDirectory()) return []
    } catch {
        return []
    }

    const entries = await readdir(targetPath, { withFileTypes: true })
    const fingerprints: string[] = []

    for (const entry of entries) {
        fingerprints.push(...await pathFingerprint(path.join(targetPath, entry.name)))
    }

    return fingerprints
}

async function buildFingerprint() {
    const entries = await Promise.all([
        pathFingerprint(Router.routesPath),
        pathFingerprint(Router.componentsPath),
        pathFingerprint(Router.assetsPath),
        pathFingerprint(path.join(process.cwd(), 'main.js')),
        pathFingerprint(path.join(process.cwd(), 'package.json'))
    ])

    return entries.flat().sort().join('|')
}

async function runFreshBundleBuild() {
    const proc = Bun.spawn(['bun', bundleCliPath], {
        cwd: process.cwd(),
        env: process.env,
        stdout: 'inherit',
        stderr: 'inherit'
    })

    const exitCode = await proc.exited
    if (exitCode !== 0) {
        throw new Error(`Bundle subprocess exited with code ${exitCode}`)
    }
}

async function startPreviewBundleWatcher(): Promise<PreviewWatcherHandle> {
    await runFreshBundleBuild()

    let lastFingerprint = await buildFingerprint()
    let building = false
    let queued = false

    const runQueuedBuild = async () => {
        if (building) {
            queued = true
            return
        }

        building = true
        try {
            await runFreshBundleBuild()
            lastFingerprint = await buildFingerprint()
        } catch (err) {
            console.error(`Preview rebuild failed: ${(err as Error).message}`, process.pid)
        } finally {
            building = false
            if (queued) {
                queued = false
                await runQueuedBuild()
            }
        }
    }

    const timer = setInterval(async () => {
        const nextFingerprint = await buildFingerprint()
        if (nextFingerprint === lastFingerprint) return
        await runQueuedBuild()
    }, WATCH_INTERVAL_MS)

    return {
        close() {
            clearInterval(timer)
        }
    }
}

let bundleWatcher: PreviewWatcherHandle | null = null

if (watchMode) {
    bundleWatcher = await startPreviewBundleWatcher()
} else {
    try {
        await access(distPath)
    } catch {
        console.error(`Missing dist directory at '${distPath}'. Run 'tach.bundle' first.`, process.pid)
        process.exit(1)
    }
}

const start = Date.now()
const server = await createStaticPreviewServer(distPath)

console.info(`Preview running on http://${server.hostname}:${server.port} — started in ${Date.now() - start}ms`, process.pid)

for (const signal of ['SIGINT', 'SIGTERM'] as const) {
    process.on(signal, () => {
        bundleWatcher?.close()
        server.stop()
        process.exit(0)
    })
}
