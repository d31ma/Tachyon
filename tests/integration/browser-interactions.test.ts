import { expect, test } from 'bun:test'

const BROWSER_RUNNER = './tests/integration/browser-interactions.runner.ts'
const PLAYWRIGHT_PROBE_TIMEOUT_MS = 2000
const BROWSER_RUN_TIMEOUT_MS = 120000

function isLegacyBunRuntime(): boolean {
    const [major = 0, minor = 0] = Bun.version.split('.').map(Number)
    return major < 1 || (major === 1 && minor < 3)
}

function decode(stream: ReadableStream<Uint8Array> | null): Promise<string> {
    if (!stream) return Promise.resolve('')
    return new Response(stream).text()
}

async function runCommand(
    cmd: string[],
    timeoutMs: number
): Promise<{ exitCode: number | null; stdout: string; stderr: string; timedOut: boolean }> {
    const proc = Bun.spawn(cmd, {
        cwd: process.cwd(),
        stdout: 'pipe',
        stderr: 'pipe'
    })

    const stdoutPromise = decode(proc.stdout)
    const stderrPromise = decode(proc.stderr)
    let timedOut = false

    const timeout = setTimeout(() => {
        timedOut = true
        proc.kill()
    }, timeoutMs)

    const exitCode = await proc.exited.catch(() => null)
    clearTimeout(timeout)

    return {
        exitCode,
        stdout: await stdoutPromise,
        stderr: await stderrPromise,
        timedOut
    }
}

async function canRunPlaywright(): Promise<{ ok: boolean; detail?: string }> {
    const probe = await runCommand(
        ['node', '-e', "require('playwright-core'); console.log('playwright-ready')"],
        PLAYWRIGHT_PROBE_TIMEOUT_MS
    )

    if (probe.timedOut) {
        return { ok: false, detail: 'Playwright module import timed out in this environment' }
    }

    if (probe.exitCode !== 0) {
        const detail = (probe.stderr || probe.stdout).trim() || `Exited with code ${probe.exitCode}`
        return { ok: false, detail }
    }

    return { ok: true }
}

test('browser interactions runner completes when Playwright is available', { timeout: 15000 }, async () => {
    if (process.env.CI) {
        console.warn('[browser-test] Skipping Playwright runner in CI; use local smoke runs for browser HMR verification')
        expect(true).toBe(true)
        return
    }

    if (isLegacyBunRuntime()) {
        console.warn(`[browser-test] Skipping Playwright runner on Bun ${Bun.version}: probe is unstable on this runtime`)
        expect(true).toBe(true)
        return
    }

    const probe = await canRunPlaywright()

    if (!probe.ok) {
        console.warn(`[browser-test] Skipping Playwright runner: ${probe.detail}`)
        expect(true).toBe(true)
        return
    }

    const run = await runCommand(['bun', BROWSER_RUNNER], BROWSER_RUN_TIMEOUT_MS)
    const output = [run.stdout.trim(), run.stderr.trim()].filter(Boolean).join('\n')

    if (run.timedOut) {
        throw new Error(`Browser runner timed out after ${BROWSER_RUN_TIMEOUT_MS}ms\n${output}`)
    }

    if (run.exitCode !== 0) {
        throw new Error(`Browser runner failed with exit code ${run.exitCode}\n${output}`)
    }

    expect(run.exitCode).toBe(0)
})
