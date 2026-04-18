import { fileURLToPath } from 'node:url'

export async function runCli(relativePath) {
    const cliPath = fileURLToPath(new URL(relativePath, import.meta.url))
    const proc = Bun.spawn(['bun', cliPath, ...process.argv.slice(2)], {
        cwd: process.cwd(),
        env: process.env,
        stdin: 'inherit',
        stdout: 'inherit',
        stderr: 'inherit',
    })

    for (const signal of ['SIGINT', 'SIGTERM']) {
        process.on(signal, () => {
            proc.kill(signal)
        })
    }

    process.exit(await proc.exited)
}
