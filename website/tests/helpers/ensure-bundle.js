// @ts-check
import { mkdir, rm, stat } from 'node:fs/promises'

const PROJECT_ROOT = import.meta.dir.replace(/[/\\]tests[/\\]helpers$/, '')
const LOCK_DIR = `${PROJECT_ROOT}/.test-bundle-lock`
const WEB_DIST = `${PROJECT_ROOT}/dist/web`
const REQUIRED_OUTPUTS = [
    `${WEB_DIST}/index.html`,
    `${WEB_DIST}/atlas/index.html`,
    `${WEB_DIST}/atlas/overview/index.html`,
    `${WEB_DIST}/atlas/connect/index.html`,
    `${WEB_DIST}/docs/index.html`,
    `${WEB_DIST}/.tachyon/tac-client.js`,
    `${WEB_DIST}/.tachyon/components/language-javascript.js`,
    `${WEB_DIST}/.tachyon/components/language-dart.mjs`,
    `${WEB_DIST}/.tachyon/components/language-kotlin.mjs`,
    `${WEB_DIST}/.tachyon/components/language-swift.wasm`,
    `${WEB_DIST}/.tachyon/components/language-csharp.mjs`,
    `${WEB_DIST}/.tachyon/components/panel-portablebridge.wasm`,
]

/** @type {Promise<void> | null} */
let bundlePromise = null

/** @param {string} path */
async function fileExists(path) {
    try {
        await stat(path)
        return true
    } catch {
        return false
    }
}

async function outputsReady() {
    const checks = await Promise.all(REQUIRED_OUTPUTS.map(fileExists))
    return checks.every(Boolean)
}

async function waitForUnlock(timeoutMs = 120_000) {
    const startedAt = Date.now()
    while (Date.now() - startedAt < timeoutMs) {
        if (!(await fileExists(LOCK_DIR)) && (await outputsReady())) return
        await Bun.sleep(250)
    }
    throw new Error('Timed out waiting for the Rust website build lock to clear')
}

/** @param {'cargo' | 'rustc'} tool */
async function rustupWhich(tool) {
    const proc = Bun.spawn(['rustup', 'which', '--toolchain', '1.97.1', tool], {
        stdout: 'pipe',
        stderr: 'pipe',
    })
    const [stdout, stderr, exitCode] = await Promise.all([
        new Response(proc.stdout).text(),
        new Response(proc.stderr).text(),
        proc.exited,
    ])
    if (exitCode !== 0)
        throw new Error(`Unable to locate Rust 1.97.1 ${tool}: ${stderr}`.trim())
    return stdout.trim()
}

async function runBundle() {
    const configured = process.env.TAC_BIN
    const releaseBinary = `${PROJECT_ROOT}/../target/release/ty`
    const debugBinary = `${PROJECT_ROOT}/../target/debug/ty`
    const command = configured
        ? [configured, 'bundle', '.']
        : await fileExists(releaseBinary)
            ? [releaseBinary, 'bundle', '.']
            : await fileExists(debugBinary)
                ? [debugBinary, 'bundle', '.']
                : [await rustupWhich('cargo'), 'run', '--release', '--locked', '--manifest-path', '../Cargo.toml', '--', 'bundle', '.']
    const rustc = configured || await fileExists(releaseBinary) || await fileExists(debugBinary)
        ? process.env.RUSTC
        : await rustupWhich('rustc')
    const proc = Bun.spawn(command, {
        cwd: PROJECT_ROOT,
        stdout: 'pipe',
        stderr: 'pipe',
        env: { ...process.env, ...(rustc ? { RUSTC: rustc } : {}) },
    })
    const [stdout, stderr, exitCode] = await Promise.all([
        new Response(proc.stdout).text(),
        new Response(proc.stderr).text(),
        proc.exited,
    ])
    if (exitCode !== 0) {
        throw new Error(`Rust website build failed with exit code ${exitCode}\n${stdout}\n${stderr}`.trim())
    }
}

export async function ensureBundle() {
    if (bundlePromise) return bundlePromise
    bundlePromise = (async () => {
        try {
            await mkdir(LOCK_DIR)
            await runBundle()
        } catch (error) {
            if (error && typeof error === 'object' && /** @type {{ code?: string }} */ (error).code === 'EEXIST') {
                await waitForUnlock()
                return
            }
            throw error
        } finally {
            await rm(LOCK_DIR, { recursive: true, force: true })
        }
    })()
    return bundlePromise
}
