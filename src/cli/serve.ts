#!/usr/bin/env bun
import Tach from "../server/process-executor.js"
import Pool from "../server/process-pool.js"
import Router from "../server/route-handler.js"
import Yon from "../compiler/template-compiler.js"
import logger from "../server/logger.js"
import { watch, type FSWatcher } from "fs"
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
const hmrClients = new Set<ReadableStreamDefaultController<string>>()
const hmrMaxClients = Number(process.env.HMR_MAX_CLIENTS) || 20
const hmrWatchers: FSWatcher[] = []
let hmrWatchersStarted = false

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

if (process.env.NODE_ENV === 'production' && !fullModeEnabled) {
    serveLogger.warn('Production mode without --full will serve the production shell fallback only; run tach.serve --full to serve bundled frontend assets')
}

let debounceTimer: Timer

function isLoopbackHost(hostname: string | null | undefined): boolean {
    return hostname === '127.0.0.1' || hostname === 'localhost' || hostname === '::1'
}

function isAuthorizedHmrRequest(req: Request, hostname: string | null | undefined): boolean {
    if (isLoopbackHost(hostname)) return true

    const token = process.env.HMR_TOKEN || process.env.DEV_TOKEN
    if (!token) return false

    const url = new URL(req.url)
    const provided = req.headers.get('X-Tachyon-Dev-Token') || url.searchParams.get('token')

    return provided === token
}

async function startHmrWatchers(server: ReturnType<typeof Bun.serve>) {
    if (hmrWatchersStarted) return
    hmrWatchersStarted = true

    const onFileChange = () => {
        clearTimeout(debounceTimer)
        debounceTimer = setTimeout(async () => {
            try {
                serveLogger.info('HMR reload started')
                await configureRoutes(true)
                server.reload({ routes: Router.reqRoutes })
                for (const client of hmrClients) client.enqueue("event: reload\ndata: reload\n\n")
            } catch (err) {
                serveLogger.error('HMR reload failed', { err })
            }
        }, HMR_DEBOUNCE_MS)
    }

    if (await pathExists(Router.routesPath)) {
        hmrWatchers.push(watch(Router.routesPath, { recursive: true }, onFileChange))
    }

    if (await pathExists(Router.componentsPath)) {
        hmrWatchers.push(watch(Router.componentsPath, { recursive: true }, onFileChange))
    }
}

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

        if (!isAuthorizedHmrRequest(req, server.hostname)) {
            return new Response("Forbidden", { status: 403 })
        }

        if (hmrClients.size >= hmrMaxClients) {
            return new Response("Too Many HMR Clients", { status: 429 })
        }

        server.timeout(req, 0)

        let hmrController: ReadableStreamDefaultController<string> | null = null

        return new Response(new ReadableStream({
            async start(controller) {
                hmrController = controller
                hmrClients.add(controller)
                controller.enqueue(": connected\n\n")
                await startHmrWatchers(server)
            },
            cancel() {
                if (hmrController) hmrClients.delete(hmrController)
            },
        }), {
            headers: {
                "Content-Type": "text/event-stream",
                "Cache-Control": "no-cache",
                "Connection": "keep-alive",
            }
        })
    },

    routes:      Router.reqRoutes,
    port:        process.env.PORT     || 8080,
    hostname:    process.env.HOST || process.env.HOSTNAME || '127.0.0.1',
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
    for (const watcher of hmrWatchers) watcher.close()
    Pool.clearWarmedProcesses()
    bundleWatcher?.kill()
    server.stop()
    serveLogger.info('Server stopped', { signal: 'SIGINT' })
    process.exit(0)
})

process.on('SIGTERM', () => {
    clearTimeout(debounceTimer)
    for (const watcher of hmrWatchers) watcher.close()
    Pool.clearWarmedProcesses()
    bundleWatcher?.kill()
    server.stop()
    serveLogger.info('Server stopped', { signal: 'SIGTERM' })
    process.exit(0)
})
