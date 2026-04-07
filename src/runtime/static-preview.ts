import { stat } from 'node:fs/promises'
import path from 'node:path'

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
    const body = await file.bytes()

    return new Response(body, {
        headers: file.type ? { 'Content-Type': file.type } : undefined
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
