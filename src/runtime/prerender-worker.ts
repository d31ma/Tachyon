#!/usr/bin/env bun
import Yon from '../compiler/template-compiler.js'

interface PrerenderPayload {
    distPath: string
    pathname: string
    shellHTML: string
    layoutMapping: Record<string, string>
}

const payload = await Bun.stdin.json() as PrerenderPayload
const html = await Yon.renderPageDocumentForWorker(
    payload.distPath,
    payload.pathname,
    payload.shellHTML,
    payload.layoutMapping
)

Bun.stdout.write(html)
