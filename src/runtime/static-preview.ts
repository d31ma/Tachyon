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

export async function resolvePreviewFile(distPath: string, pathname: string) {
    const normalized = pathname === '/' ? '/' : pathname.replace(/\/+$/, '') || '/'

    const directFile = path.join(distPath, normalized === '/' ? 'index.html' : normalized.slice(1))
    if (await pathExists(directFile)) return directFile

    if (!shouldTreatAsAsset(normalized)) {
        const nestedIndex = path.join(distPath, normalized === '/' ? 'index.html' : normalized.slice(1), 'index.html')
        if (await pathExists(nestedIndex)) return nestedIndex

        const rootIndex = path.join(distPath, 'index.html')
        if (await pathExists(rootIndex)) return rootIndex
    }

    return null
}

export async function createStaticPreviewServer(
    distPath: string,
    options: { port?: number; hostname?: string } = {}
) {
    const server = Bun.serve({
        port: options.port ?? Number(process.env.PORT || 3000),
        hostname: options.hostname ?? process.env.HOSTNAME ?? '0.0.0.0',
        async fetch(req) {
            const url = new URL(req.url)
            const filePath = await resolvePreviewFile(distPath, url.pathname)

            if (!filePath) return new Response('Not Found', { status: 404 })

            const file = Bun.file(filePath)
            const body = await file.bytes()
            return new Response(body, {
                headers: file.type ? { 'Content-Type': file.type } : undefined
            })
        }
    })

    return server
}
