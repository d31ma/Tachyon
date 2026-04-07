declare var self: Worker;

interface WorkerMessage {
    script: string
    /** Optional working directory for the spawned server process */
    cwd?: string
    /** Optional environment variables for the spawned server process */
    env?: Record<string, string>
}

let proc: Bun.Subprocess | null = null

self.onmessage = async (event: MessageEvent<WorkerMessage | string>) => {
    if (event.data === 'stop') {
        proc?.kill()
        proc = null
        self.close()
        return
    }

    const { script, cwd } = typeof event.data === 'string'
        ? { script: event.data, cwd: undefined, env: undefined }
        : event.data

    // Resolve the script path relative to the project root before changing cwd
    const absoluteScript = Bun.resolveSync(script, process.cwd())
    proc?.kill()
    proc = Bun.spawn(['bun', absoluteScript], {
        cwd: cwd ? Bun.resolveSync(cwd, process.cwd()) : process.cwd(),
        env: env ? { ...process.env, ...env } : process.env,
        stdout: 'inherit',
        stderr: 'inherit'
    })

    await proc.exited
}
