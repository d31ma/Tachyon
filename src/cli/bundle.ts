#!/usr/bin/env bun
import Router from "../server/route-handler.js"
import Yon from "../compiler/template-compiler.js"
import logger from "../server/logger.js"
import { access, mkdir, readdir, rm, stat } from "node:fs/promises"
import { watch, type FSWatcher } from "node:fs"
import path from "node:path"

const distPath = `${process.cwd()}/dist`
const watchMode = process.argv.includes('--watch')
const WATCH_DEBOUNCE_MS = 200
const BUILD_CONCURRENCY = 8
const bundleLogger = logger.child({ scope: 'cli:bundle' })

async function runWithConcurrency<T>(
    items: T[],
    limit: number,
    worker: (item: T) => Promise<void>
) {
    if (items.length === 0) return

    let index = 0

    const runners = Array.from({ length: Math.min(limit, items.length) }, async () => {
        while (index < items.length) {
            const current = items[index++]
            await worker(current)
        }
    })

    await Promise.all(runners)
}

async function buildRouteOutput(route: string) {
    // Skip the HMR script — it is a dev-only asset
    if (route.includes('hot-reload-client')) return

    const handler = Router.reqRoutes[route]?.GET
    if (!handler) return

    try {
        const res = await handler()
        await Bun.write(Bun.file(`${distPath}${route}`), await res.blob())
    } catch (err) {
        bundleLogger.error('Failed to build route', { route, err })
    }
}

export async function runBuild() {
    const start = Date.now()

    Router.resetStaticState()

    await rm(distPath, { recursive: true, force: true })
    await mkdir(distPath, { recursive: true })

    await Yon.createStaticRoutes()

    await runWithConcurrency(Object.keys(Router.reqRoutes), BUILD_CONCURRENCY, buildRouteOutput)

    await Yon.prerenderStaticPages(distPath)

    bundleLogger.info('Bundle completed', {
        durationMs: Date.now() - start,
        routeCount: Object.keys(Router.reqRoutes).length,
        distPath,
    })
}

async function writeRouteOutput(route: string) {
    await buildRouteOutput(route)
}

async function pathExists(filePath: string) {
    try {
        await access(filePath)
        return true
    } catch {
        return false
    }
}

type ChangeKind = {
    type: 'full' | 'page' | 'layout' | 'component' | 'asset' | 'main'
    relative: string
}

function normalizeRelative(filePath: string) {
    return filePath.replaceAll(path.sep, '/').replace(/^\.\//, '')
}

function classifyChange(targetPath: string): ChangeKind {
    const relative = normalizeRelative(path.relative(process.cwd(), targetPath))

    if (relative === 'main.js') return { type: 'main', relative }
    if (relative === 'package.json') return { type: 'full', relative }

    const routesPrefix = normalizeRelative(path.relative(process.cwd(), Router.routesPath)) + '/'
    const componentsPrefix = normalizeRelative(path.relative(process.cwd(), Router.componentsPath)) + '/'
    const assetsPrefix = normalizeRelative(path.relative(process.cwd(), Router.assetsPath)) + '/'

    if (relative.startsWith(routesPrefix)) {
        const routeFile = relative.slice(routesPrefix.length)
        if (routeFile.endsWith('/HTML') || routeFile === 'HTML') return { type: 'page', relative: routeFile }
        if (routeFile.endsWith('/LAYOUT') || routeFile === 'LAYOUT') return { type: 'layout', relative: routeFile }
        return { type: 'full', relative }
    }

    if (relative.startsWith(componentsPrefix)) {
        const componentFile = relative.slice(componentsPrefix.length)
        if (componentFile.endsWith('.html')) return { type: 'component', relative: componentFile }
        return { type: 'full', relative }
    }

    if (relative.startsWith(assetsPrefix)) {
        return { type: 'asset', relative: relative.slice(assetsPrefix.length) }
    }

    return { type: 'full', relative }
}

function routesUsingLayout(layoutRoute: string) {
    const prefix = layoutRoute === 'LAYOUT' ? '/' : `/${layoutRoute.replace(/\/LAYOUT$/, '')}`
    return Yon.getHtmlRoutes().filter((route) => route === prefix || route.startsWith(`${prefix}/`) || prefix === '/')
}

async function runSelectiveBuild(change: ChangeKind) {
    const start = Date.now()
    const logIncrementalBuild = () => bundleLogger.info('Incremental build completed', {
        changeType: change.type,
        target: change.relative,
        durationMs: Date.now() - start,
    })

    if (change.type === 'main') {
        const mainPath = `${process.cwd()}/main.js`
        if (!await pathExists(mainPath)) {
            delete Router.reqRoutes['/main.js']
            await rm(`${distPath}/main.js`, { force: true })
        } else {
            Router.reqRoutes['/main.js'] = {
                GET: async () => new Response(await Bun.file(mainPath).bytes(), {
                    headers: { 'Content-Type': 'application/javascript' }
                })
            }
            await writeRouteOutput('/main.js')
        }
        logIncrementalBuild()
        return
    }

    if (change.type === 'asset') {
        const sourcePath = path.join(Router.assetsPath, change.relative)
        const outputPath = path.join(distPath, 'assets', change.relative)

        if (!await pathExists(sourcePath)) {
            delete Router.reqRoutes[`/assets/${change.relative}`]
            await rm(outputPath, { force: true })
        } else {
            await Yon.bundleAssetFile(change.relative)
            await writeRouteOutput(`/assets/${change.relative}`)
        }

        logIncrementalBuild()
        return
    }

    if (change.type === 'component') {
        const sourcePath = path.join(Router.componentsPath, change.relative)
        const outputRoute = `/components/${change.relative.replace('.html', '.js')}`

        if (!await pathExists(sourcePath)) {
            delete Router.reqRoutes[outputRoute]
            await rm(`${distPath}${outputRoute}`, { force: true })
            await runBuild()
            return
        }

        await Yon.bundleComponentFile(change.relative)
        await writeRouteOutput(outputRoute)
        await Yon.prerenderRoutes(distPath, Yon.getHtmlRoutes())
        logIncrementalBuild()
        return
    }

    if (change.type === 'page') {
        const sourcePath = path.join(Router.routesPath, change.relative)
        const outputRoute = `/pages/${change.relative}.js`
        const routePath = change.relative === 'HTML'
            ? '/'
            : `/${change.relative.replace(/\/HTML$/, '')}`

        if (!await pathExists(sourcePath)) {
            delete Router.reqRoutes[outputRoute]
            await rm(`${distPath}${outputRoute}`, { force: true })
            await rm(routePath === '/' ? `${distPath}/index.html` : `${distPath}${routePath}/index.html`, { force: true })
            await runBuild()
            return
        }

        await Yon.bundlePageFile(change.relative)
        await writeRouteOutput(outputRoute)
        await Yon.prerenderRoutes(distPath, [routePath])
        logIncrementalBuild()
        return
    }

    if (change.type === 'layout') {
        const sourcePath = path.join(Router.routesPath, change.relative)
        const outputRoute = `/layouts/${change.relative}.js`

        if (!await pathExists(sourcePath)) {
            await runBuild()
            return
        }

        await Yon.bundleLayoutFile(change.relative)
        await writeRouteOutput(outputRoute)
        await Yon.prerenderRoutes(distPath, routesUsingLayout(change.relative))
        logIncrementalBuild()
        return
    }

    await runBuild()
}

async function collectDirectories(root: string, results: string[] = []) {
    try {
        const info = await stat(root)
        if (!info.isDirectory()) return results
    } catch {
        return results
    }

    results.push(root)

    for (const entry of await readdir(root, { withFileTypes: true })) {
        if (entry.isDirectory()) {
            await collectDirectories(path.join(root, entry.name), results)
        }
    }

    return results
}

async function watchPaths(onChange: (targetPath: string, eventType: string) => void) {
    const watchers = new Map<string, FSWatcher>()
    const roots = [
        Router.routesPath,
        Router.componentsPath,
        Router.assetsPath,
    ]

    const addWatcher = (targetPath: string, kind: 'file' | 'directory') => {
        if (watchers.has(targetPath)) return

        try {
            const watcher = watch(targetPath, { persistent: true }, (eventType, filename) => {
                const changedPath = filename && kind === 'directory'
                    ? path.resolve(targetPath, filename.toString())
                    : targetPath
                onChange(changedPath, eventType)
            })
            watchers.set(targetPath, watcher)
        } catch {
            // ignore paths the platform cannot watch directly
        }
    }

    const syncDirectoryWatchers = async () => {
        const active = new Set<string>()
        for (const root of roots) {
            for (const dir of await collectDirectories(root)) {
                active.add(dir)
                addWatcher(dir, 'directory')
            }
        }

        for (const file of [`${process.cwd()}/main.js`, `${process.cwd()}/package.json`]) {
            active.add(file)
            addWatcher(file, 'file')
        }

        for (const [target, watcher] of watchers.entries()) {
            if (!active.has(target)) {
                watcher.close()
                watchers.delete(target)
            }
        }
    }

    await syncDirectoryWatchers()

    return {
        async refresh() {
            await syncDirectoryWatchers()
        },
        close() {
            for (const watcher of watchers.values()) watcher.close()
            watchers.clear()
        }
    }
}

export type BundleWatcherHandle = {
    close(): void
}

export async function startBundleWatcher(options: { incremental?: boolean } = {}): Promise<BundleWatcherHandle> {
    const incremental = options.incremental ?? true
    await runBuild()

    let debounceTimer: Timer | undefined
    let building = false
    let queued = false

    let pendingChange: ChangeKind | null = null

    const mergeChanges = (current: ChangeKind | null, next: ChangeKind): ChangeKind => {
        if (!current) return next
        if (current.type === 'full' || next.type === 'full') return { type: 'full', relative: next.relative }
        if (current.type === next.type && current.relative === next.relative) return current
        return { type: 'full', relative: next.relative }
    }

    const schedule = (targetPath: string, eventType: string) => {
        clearTimeout(debounceTimer)
        const nextChange = eventType === 'rename'
            ? { type: 'full', relative: normalizeRelative(path.relative(process.cwd(), targetPath)) } as ChangeKind
            : classifyChange(targetPath)
        pendingChange = mergeChanges(pendingChange, nextChange)
        debounceTimer = setTimeout(async () => {
            if (building) {
                queued = true
                return
            }

            building = true
            try {
                await watcher.refresh()
                const change = pendingChange ?? { type: 'full', relative: 'unknown' }
                pendingChange = null
                if (incremental) {
                    await runSelectiveBuild(change)
                } else {
                    await runBuild()
                }
            } catch (err) {
                bundleLogger.error('Watch rebuild failed', {
                    changedPath: targetPath,
                    eventType,
                    err,
                })
            } finally {
                building = false
                if (queued) {
                    queued = false
                    pendingChange = mergeChanges(
                        pendingChange,
                        { type: 'full', relative: 'queued-change' }
                    )
                    schedule(process.cwd(), 'change')
                }
            }
        }, WATCH_DEBOUNCE_MS)
    }

    const watcher = await watchPaths(schedule)

    bundleLogger.info('Watching source paths for bundle changes', {
        routesPath: Router.routesPath,
        componentsPath: Router.componentsPath,
        assetsPath: Router.assetsPath,
    })

    return {
        close() {
            clearTimeout(debounceTimer)
            watcher.close()
        }
    }
}

if (import.meta.main) {
    if (!watchMode) {
        await runBuild()
    } else {
        const bundleWatcher = await startBundleWatcher()

        const shutdown = () => {
            bundleWatcher.close()
            process.exit(0)
        }

        process.on('SIGINT', shutdown)
        process.on('SIGTERM', shutdown)

        await new Promise(() => {})
    }
}
