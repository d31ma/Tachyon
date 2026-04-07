#!/usr/bin/env bun
import Tach from "../server/process-executor.js"
import Pool from "../server/process-pool.js"
import Router from "../server/route-handler.js"
import Yon from "../compiler/template-compiler.js"
import "../server/console-logger.js"
import { watch } from "fs"
import { access } from "fs/promises"
import type { Middleware } from "../server/route-handler.js"

/** Debounce delay (ms) applied to file-watcher events before triggering an HMR reload */
const HMR_DEBOUNCE_MS = 1000
const bundleWatchEnabled = process.argv.includes('--bundle-watch')
const fullModeEnabled = process.argv.includes('--full')

const start = Date.now()
let bundleWatcher: Bun.Subprocess | null = null
let previewWatcher: Bun.Subprocess | null = null

async function pathExists(path: string): Promise<boolean> {
    try { await access(path); return true } catch { return false }
}

async function loadMiddleware() {
    const extensions = ['.ts', '.js']
    for (const ext of extensions) {
        const filePath = `${Router.middlewarePath}${ext}`
        if (await pathExists(filePath)) {
            const mod = await import(filePath)
            const loaded = mod.default ?? mod
            if (typeof loaded !== 'object' || loaded === null ||
                (loaded.before !== undefined && typeof loaded.before !== 'function') ||
                (loaded.after  !== undefined && typeof loaded.after  !== 'function')) {
                throw new Error(`Middleware at '${filePath}' must export an object with optional before/after functions`)
            }
            Router.middleware = loaded as Middleware
            return
        }
    }
    Router.middleware = null
}

async function configureRoutes(isReload = false) {
    if (isReload) Pool.clearWarmedProcesses()
    await loadMiddleware()
    await Router.validateRoutes()
    Tach.createServerRoutes()
    Pool.prewarmAllHandlers()
    await Yon.createStaticRoutes()
}

await configureRoutes()

if (bundleWatchEnabled) {
    bundleWatcher = Bun.spawn(
        ['bun', `${import.meta.dir}/bundle.ts`, '--watch'],
        {
            cwd: process.cwd(),
            stdout: 'inherit',
            stderr: 'inherit'
        }
    )
}

if (fullModeEnabled) {
    const previewPort = process.env.PREVIEW_PORT || '3000'
    const previewHost = process.env.PREVIEW_HOST || process.env.HOST || process.env.HOSTNAME || '127.0.0.1'

    previewWatcher = Bun.spawn(
        ['bun', `${import.meta.dir}/preview.ts`, '--watch'],
        {
            cwd: process.cwd(),
            env: {
                ...process.env,
                PORT: previewPort,
                HOST: previewHost,
            },
            stdout: 'inherit',
            stderr: 'inherit'
        }
    )
}

let debounceTimer: Timer

const server = Bun.serve({
    idleTimeout: process.env.TIMEOUT ? Number(process.env.TIMEOUT) : 0,

    fetch(req, server) {

        if (new URL(req.url).pathname !== "/hmr") {
            return new Response("Not Found", { status: 404 })
        }

        server.timeout(req, 0)

        return new Response(new ReadableStream({
            async start(controller) {

                const onFileChange = () => {
                    clearTimeout(debounceTimer)
                    debounceTimer = setTimeout(async () => {
                        try {
                            console.info("HMR Update", process.pid)
                            await configureRoutes(true)
                            server.reload({ routes: Router.reqRoutes })
                            controller.enqueue("\n\n")
                        } catch (err) {
                            console.error(`HMR reload failed: ${(err as Error).message}`, process.pid)
                        }
                    }, HMR_DEBOUNCE_MS)
                }

                if (await pathExists(Router.routesPath))     watch(Router.routesPath,     { recursive: true }, onFileChange)
                if (await pathExists(Router.componentsPath)) watch(Router.componentsPath, { recursive: true }, onFileChange)
            }
        }), { headers: { "Content-Type": "text/event-stream" } })
    },

    routes:      Router.reqRoutes,
    port:        process.env.PORT     || 8080,
    hostname:    process.env.HOST || process.env.HOSTNAME || '0.0.0.0',
    development: !!process.env.DEV,
})

console.info(`Server running on http://${server.hostname}:${server.port} — started in ${Date.now() - start}ms`, process.pid)

process.on('SIGINT', () => {
    clearTimeout(debounceTimer)
    Pool.clearWarmedProcesses()
    bundleWatcher?.kill()
    previewWatcher?.kill()
    server.stop()
    process.exit(0)
})

process.on('SIGTERM', () => {
    clearTimeout(debounceTimer)
    Pool.clearWarmedProcesses()
    bundleWatcher?.kill()
    previewWatcher?.kill()
    server.stop()
    process.exit(0)
})
