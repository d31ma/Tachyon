// @ts-check
import { chmod, lstat, mkdir, rm, stat } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { tmpdir, userInfo } from 'node:os'
import { join, resolve } from 'node:path'

const PROJECT_ROOT = resolve(import.meta.dir, '../..')
const user = process.getuid?.() ?? createHash('sha256').update(userInfo().username).digest('hex').slice(0, 12)
const LOCK_ROOT = join(tmpdir(), `tachyon-website-tests-${user}`)
// A synced folder can recreate a deleted empty lock. Keep coordination local
// to this user and distinguish independent checkouts by their absolute path.
const LOCK_DIR = join(LOCK_ROOT, createHash('sha256').update(PROJECT_ROOT).digest('hex').slice(0, 24))
const WEB_DIST = `${PROJECT_ROOT}/dist/web`
const REQUIRED_OUTPUTS = [
    `${WEB_DIST}/index.html`,
    `${WEB_DIST}/docs/index.html`,
    `${WEB_DIST}/docs/_topic/index.html`,
    `${WEB_DIST}/docs/features/index.html`,
    `${WEB_DIST}/docs/features/_id/index.html`,
    `${WEB_DIST}/shared/data/languages.json`,
    `${WEB_DIST}/route-manifest.json`,
    `${WEB_DIST}/sitemap.xml`,
    `${WEB_DIST}/.tachyon/tac-client.js`,
]

/** @type {Promise<void> | null} */
let bundlePromise = null

async function prepareLockRoot() {
    await mkdir(LOCK_ROOT, { recursive: true, mode: 0o700 })
    const info = await lstat(LOCK_ROOT)
    if (info.isSymbolicLink() || !info.isDirectory())
        throw new Error('The website test lock root must be a real directory')
    if (process.getuid && info.uid !== process.getuid())
        throw new Error('The website test lock root must belong to this user')
    if (process.platform !== 'win32') await chmod(LOCK_ROOT, 0o700)
}

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
        let ownsLock = false
        await prepareLockRoot()
        try {
            await mkdir(LOCK_DIR, { mode: 0o700 })
            ownsLock = true
            await runBundle()
        } catch (error) {
            if (error && typeof error === 'object' && /** @type {{ code?: string }} */ (error).code === 'EEXIST') {
                await waitForUnlock()
                return
            }
            throw error
        } finally {
            if (ownsLock) await rm(LOCK_DIR, { recursive: true, force: true })
        }
    })()
    return bundlePromise
}
