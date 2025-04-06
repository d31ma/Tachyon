#!/usr/bin/env bun
import Tach from "./server/tach.js"
import Router, { _ctx } from "./router.js"
import Yon from "./client/yon.js"
import { Logger } from "./server/logger.js"
import { ServerWebSocket } from "bun"
import { watch, exists } from "fs/promises"
import { watch as watcher } from "node:fs";

type WebSocketData = {
    handler: string,
    ctx: _ctx,
    path: string
}

const start = Date.now()

Logger()

async function configureRoutes() {
    await Router.validateRoutes()
    Tach.createServerRoutes()
    await Yon.createStaticRoutes()
}

await configureRoutes()

const server = Bun.serve({
    routes: Router.reqRoutes,
    websocket: {
        async open(ws: ServerWebSocket<WebSocketData>) {

            const { handler, path } = ws.data

            const proc = Bun.spawn({
                cmd: [handler],
                stdout: 'inherit',
                stderr: "pipe",
                stdin: "pipe"
            })
 
            Tach.webSockets.set(ws, proc)

            console.info(`WebSocket Connected - ${path} - ${proc.pid}`)

            for await(const ev of watch(`/tmp`)) {

                if(ev.filename === proc.pid.toString()) {

                    const status = ws.send(Bun.mmap(`/tmp/${proc.pid}`))

                    console.info(`WebSocket Message Sent - ${path} - ${proc.pid} - ${status} byte(s)`)
                }
            }
        },
        async message(ws: ServerWebSocket<WebSocketData>, message: string) {

            const proc = Tach.webSockets.get(ws)!

            const { ctx, path } = ws.data

            ctx.body = message

            proc.stdin.write(JSON.stringify(ctx))

            proc.stdin.flush()

            console.info(`WebSocket Message Received - ${path} - ${proc.pid} - ${message.length} byte(s)`)
        },
        close(ws, code, reason) {

            const { path } = ws.data

            const proc = Tach.webSockets.get(ws)!

            proc.stdin.end()

            Tach.webSockets.delete(ws)

            console.info(`WebSocket Disconnected - ${path} - ${proc.pid} - Code (${code}): ${reason}`)
        },
    },
    port: process.env.PORT || 8080,
    hostname: process.env.HOSTNAME || '0.0.0.0',
    development: process.env.NODE_ENV === 'development'
})

if(server.development) {

    let timeout: Timer

    let websocket: ServerWebSocket<unknown>;

    const socket = Bun.serve({
        fetch(req) {
            socket.upgrade(req)
            return undefined
        },
        websocket: {
            async open(ws) {
                console.info("HMR Enabled")
                websocket = ws
            },
            message(ws, message) {
                
            },
            close(ws, code, reason) {
                console.info(`HMR Closed ${code} ${reason}`)
            },
        },
        port: 9876
    })

    if(await exists(Router.routesPath)) {

        watcher(Router.routesPath, { recursive: true }, () => {

            if(timeout) clearTimeout(timeout)

            timeout = setTimeout(async () => {
                console.info("HMR Update")
                await configureRoutes()
                server.reload({ routes: Router.reqRoutes })
                if(websocket) websocket.send('')
            }, 1500)
        })
    }

    if(await exists(Router.componentsPath)) {

        watcher(Router.componentsPath, { recursive: true }, () => {

            if(timeout) clearTimeout(timeout)

            timeout = setTimeout(async () => {
                console.info("HMR Update")
                await configureRoutes()
                server.reload({ routes: Router.reqRoutes })
                if(websocket) websocket.send('')
            }, 1500)
        })
    }
}

const elapsed = Date.now() - start

console.info(`Live Server is running on http://${server.hostname}:${server.port} (Press CTRL+C to quit) - ${elapsed.toFixed(2)}ms`)