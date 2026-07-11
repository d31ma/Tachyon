// @ts-check
import { mkdir, rm, stat } from 'node:fs/promises'

const PROJECT_ROOT = import.meta.dir.replace(/[/\\]tests[/\\]helpers$/, '')
const LOCK_DIR = `${PROJECT_ROOT}/.test-bundle-lock`
const REQUIRED_OUTPUTS = [
    `${PROJECT_ROOT}/dist/web/index.html`,
    `${PROJECT_ROOT}/dist/web/atlas/index.html`,
    `${PROJECT_ROOT}/dist/web/atlas/overview/index.html`,
    `${PROJECT_ROOT}/dist/web/atlas/connect/index.html`,
    `${PROJECT_ROOT}/dist/web/docs/index.html`,
    `${PROJECT_ROOT}/dist/web/components/panel/polyglot/tac.js`,
    `${PROJECT_ROOT}/dist/web/components/panel/portablebridge/tac.js`,
    `${PROJECT_ROOT}/dist/web/components/language/javascript/tac.js`,
    `${PROJECT_ROOT}/dist/web/components/language/dart/tac.js`,
    `${PROJECT_ROOT}/dist/web/components/language/kotlin/tac.js`,
    `${PROJECT_ROOT}/dist/web/components/language/swift/tac.js`,
    `${PROJECT_ROOT}/dist/web/components/language/csharp/tac.js`,
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

async function waitForUnlock(timeoutMs = 90_000) {
    const startedAt = Date.now()
    while (Date.now() - startedAt < timeoutMs) {
        if (!(await fileExists(LOCK_DIR)) && (await outputsReady())) return
        await Bun.sleep(250)
    }
    throw new Error('Timed out waiting for bundle lock to clear')
}

async function runBundle() {
    const proc = Bun.spawn(['bun', 'run', 'bundle'], {
        cwd: PROJECT_ROOT,
        stdout: 'pipe',
        stderr: 'pipe',
    })

    const [stdout, stderr, exitCode] = await Promise.all([
        new Response(proc.stdout).text(),
        new Response(proc.stderr).text(),
        proc.exited,
    ])

    if (exitCode !== 0) {
        throw new Error(`bundle failed with exit code ${exitCode}\n${stdout}\n${stderr}`.trim())
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
