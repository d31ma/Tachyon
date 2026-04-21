import { stat } from 'node:fs/promises'
import path from 'node:path'
import Router from '../server/route-handler.js'

async function pathExists(filePath: string) {
    try {
        const info = await stat(filePath)
        return info.isFile()
    } catch {
        return false
    }
}

function shouldTreatAsAsset(pathname: string) {
    const basename = path.posix.basename(pathname)
    return basename.includes('.')
}

export async function resolvePreviewFile(
    distPath: string,
    pathname: string,
    options: { allowRootFallback?: boolean } = {}
) {
    const allowRootFallback = options.allowRootFallback ?? true
    const normalized = pathname === '/' ? '/' : pathname.replace(/\/+$/, '') || '/'

    const directFile = path.join(distPath, normalized === '/' ? 'index.html' : normalized.slice(1))
    if (await pathExists(directFile)) return directFile

    if (!shouldTreatAsAsset(normalized)) {
        const nestedIndex = path.join(distPath, normalized === '/' ? 'index.html' : normalized.slice(1), 'index.html')
        if (await pathExists(nestedIndex)) return nestedIndex

        if (allowRootFallback) {
            const rootIndex = path.join(distPath, 'index.html')
            if (await pathExists(rootIndex)) return rootIndex
        }
    }

    return null
}

export async function serveStaticPreviewRequest(
    distPath: string,
    req: Request,
    options: { allowRootFallback?: boolean } = {}
) {
    if (req.method !== 'GET' && req.method !== 'HEAD') return null

    const url = new URL(req.url)
    const filePath = await resolvePreviewFile(distPath, url.pathname, options)

    if (!filePath) return null

    const file = Bun.file(filePath)
    const headers = new Headers()

    if (file.type) headers.set('Content-Type', file.type)
    headers.set('Cache-Control', Router.getCacheControlHeader(url.pathname, file.type))

    const body = req.method === 'HEAD' ? null : await file.bytes()

    return new Response(body, {
        headers
    })
}

export async function createStaticPreviewServer(
    distPath: string,
    options: { port?: number; hostname?: string } = {}
) {
    const server = Bun.serve({
        port: options.port ?? Number(process.env.PORT || 3000),
        hostname: options.hostname ?? process.env.HOST ?? '127.0.0.1',
        async fetch(req) {
            return await serveStaticPreviewRequest(distPath, req)
                ?? new Response('Not Found', { status: 404 })
        }
    })

    return server
}
