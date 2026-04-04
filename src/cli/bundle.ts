#!/usr/bin/env bun
import Router from "../server/route-handler.js"
import Yon from "../compiler/template-compiler.js"
import "../server/console-logger.js"
import { mkdir } from "node:fs/promises"

const start = Date.now()

const distPath = `${process.cwd()}/dist`

await mkdir(distPath, { recursive: true })

await Yon.createStaticRoutes()

for (const route in Router.reqRoutes) {

    // Skip the HMR script — it is a dev-only asset
    if (route.includes('hot-reload-client')) continue

    const handler = Router.reqRoutes[route]['GET']

    if (!handler) continue

    try {
        const res = await handler()
        await Bun.write(Bun.file(`${distPath}${route}`), await res.blob())
    } catch (err) {
        console.error(`Failed to build route ${route}: ${(err as Error).message}`, process.pid)
    }
}

await Bun.write(
    Bun.file(`${distPath}/index.html`),
    await Bun.file(`${import.meta.dir}/../runtime/shells/production.html`).text()
)

console.info(`Built in ${Date.now() - start}ms`, process.pid)
