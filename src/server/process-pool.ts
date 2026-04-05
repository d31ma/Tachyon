import Router from "./route-handler.js"

/** A Bun subprocess with all three stdio channels opened as pipes. */
export type PipedProcess = ReturnType<typeof Bun.spawn<"pipe", "pipe", "pipe">>

/** Maximum time (ms) a handler process may run before it is killed. Default: 30 s. */
const HANDLER_TIMEOUT_MS = process.env.HANDLER_TIMEOUT_MS
    ? Number(process.env.HANDLER_TIMEOUT_MS)
    : 30_000

export default class Pool {

    /**
     * Pre-warmed handler processes keyed by absolute handler path.
     * A process is spawned ahead of time so it has already loaded its interpreter
     * and is blocking on stdin by the time a request arrives, eliminating
     * cold-start latency from the request path.
     */
    private static readonly warmedProcesses = new Map<string, PipedProcess>()

    /**
     * Pre-spawns a handler process so it is warmed and blocking on stdin
     * before the next request arrives. Called at startup and after each HMR reload.
     * @param handler - Absolute path to the handler executable
     */
    static prewarmHandler(handler: string): void {
        if (Pool.warmedProcesses.has(handler)) return

        Pool.warmedProcesses.set(handler, Bun.spawn<"pipe", "pipe", "pipe">({
            cmd: [handler],
            stdin:  "pipe",
            stdout: "pipe",
            stderr: "pipe",
            env:    process.env,
        }))
    }

    /**
     * Pre-warms one process for every non-HTML, non-OPTIONS handler discovered
     * by {@link Router.validateRoutes}. Call this after `createServerRoutes`.
     */
    static prewarmAllHandlers(): void {
        for (const [route, methods] of Router.allRoutes) {
            for (const method of methods) {
                if (method === 'HTML' || method === 'OPTIONS') continue
                Pool.prewarmHandler(`${Router.routesPath}${route}/${method}`)
            }
        }
    }

    /**
     * Clears all warmed processes. Must be called before an HMR reload so
     * stale processes (pointing at old handler files) are discarded.
     */
    static clearWarmedProcesses(): void {
        for (const proc of Pool.warmedProcesses.values()) {
            try { proc.kill() } catch { /* already exited */ }
        }
        Pool.warmedProcesses.clear()
    }

    /**
     * Returns the pre-warmed process for `handler` if one exists and is still
     * running, otherwise spawns a fresh process. Immediately schedules a
     * replacement warm process for the next request.
     *
     * A kill-on-timeout timer is armed: if the process has not exited within
     * HANDLER_TIMEOUT_MS it is killed and the event is logged.
     */
    static acquireHandler(handler: string): PipedProcess {
        const warmed = Pool.warmedProcesses.get(handler)
        Pool.warmedProcesses.delete(handler)

        // Schedule a replacement so the next request finds a warm process
        setImmediate(() => Pool.prewarmHandler(handler))

        // If the warmed process exited early (e.g. handler syntax error), spawn fresh
        const proc: PipedProcess = (warmed && warmed.exitCode === null)
            ? warmed
            : Bun.spawn<"pipe", "pipe", "pipe">({
                cmd:    [handler],
                stdin:  "pipe",
                stdout: "pipe",
                stderr: "pipe",
                env:    process.env,
            })

        // Kill hung processes to prevent resource exhaustion
        const timeout = setTimeout(() => {
            if (proc.exitCode === null) {
                console.error(`[pool] Handler timed out after ${HANDLER_TIMEOUT_MS}ms — killing process`, proc.pid)
                try { proc.kill() } catch { /* already exited */ }
            }
        }, HANDLER_TIMEOUT_MS)

        // Clear the timer once the process exits naturally
        proc.exited.then(() => clearTimeout(timeout)).catch(() => clearTimeout(timeout))

        return proc
    }
}
