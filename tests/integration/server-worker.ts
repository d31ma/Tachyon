import { $ } from 'bun'

declare var self: Worker;

interface WorkerMessage {
    script: string
    /** Optional working directory for the spawned server process */
    cwd?: string
}

self.onmessage = async (event: MessageEvent<WorkerMessage | string>) => {

    const { script, cwd } = typeof event.data === 'string'
        ? { script: event.data, cwd: undefined }
        : event.data

    // Resolve the script path relative to the project root before changing cwd
    const absoluteScript = Bun.resolveSync(script, process.cwd())

    const shell = cwd
        ? $`bun ${absoluteScript}`.cwd(cwd)
        : $`bun ${absoluteScript}`

    await shell
}
