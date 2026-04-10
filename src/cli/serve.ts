#!/usr/bin/env bun
import Tach from "../server/process-executor.js"
import Pool from "../server/process-pool.js"
import Router from "../server/route-handler.js"
import Yon from "../compiler/template-compiler.js"
import logger from "../server/logger.js"
import { watch } from "fs"
import { access } from "fs/promises"
import path from "node:path"
import type { Middleware } from "../server/route-handler.js"
import { serveStaticPreviewRequest } from "../runtime/static-preview.js"

/** Debounce delay (ms) applied to file-watcher events before triggering an HMR reload */
const HMR_DEBOUNCE_MS = 1000
const bundleWatchEnabled = process.argv.includes('--bundle-watch')
const fullModeEnabled = process.argv.includes('--full')

const start = Date.now()
let bundleWatcher: Bun.Subprocess | null = null
const distPath = path.join(process.cwd(), 'dist')
const bundleCliPath = `${import.meta.dir}/bundle.ts`
const serveLogger = logger.child({ scope: 'cli:serve' })

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
    if (!fullModeEnabled) {
        await Yon.createStaticRoutes()
    }
}

if (fullModeEnabled) {
    const { runBuild } = await import('./bundle.js')
    await runBuild()
    Router.resetStaticState()
}

await configureRoutes()

if (bundleWatchEnabled || fullModeEnabled) {
    bundleWatcher = Bun.spawn(['bun', bundleCliPath, '--watch'], {
        cwd: process.cwd(),
        stdout: 'inherit',
        stderr: 'inherit'
    })
}

if (fullModeEnabled) {
    Tach.setFrontendRequestHandler((request) => serveStaticPreviewRequest(
        distPath,
        request,
        { allowRootFallback: false }
    ))
} else {
    Tach.setFrontendRequestHandler(null)
}

let debounceTimer: Timer

const server = Bun.serve({
    idleTimeout: process.env.TIMEOUT ? Number(process.env.TIMEOUT) : 0,

    fetch(req, server) {
        if (fullModeEnabled) {
            return serveStaticPreviewRequest(distPath, req, { allowRootFallback: true })
                .then((response) => response ?? new Response("Not Found", { status: 404 }))
        }

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
                            serveLogger.info('HMR reload started')
                            await configureRoutes(true)
                            server.reload({ routes: Router.reqRoutes })
                            controller.enqueue("\n\n")
                        } catch (err) {
                            serveLogger.error('HMR reload failed', { err })
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

serveLogger.info('Server started', {
    url: `http://${server.hostname}:${server.port}`,
    startupMs: Date.now() - start,
    bundleWatchEnabled,
    fullModeEnabled,
})

process.on('SIGINT', () => {
    clearTimeout(debounceTimer)
    Pool.clearWarmedProcesses()
    bundleWatcher?.kill()
    server.stop()
    serveLogger.info('Server stopped', { signal: 'SIGINT' })
    process.exit(0)
})

process.on('SIGTERM', () => {
    clearTimeout(debounceTimer)
    Pool.clearWarmedProcesses()
    bundleWatcher?.kill()
    server.stop()
    serveLogger.info('Server stopped', { signal: 'SIGTERM' })
    process.exit(0)
})
