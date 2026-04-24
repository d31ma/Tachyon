// @ts-check
"use strict";
/**
 * @typedef {{ script: string, cwd?: string, env?: Record<string, string> }} WorkerStartMessage
 */

/** @type {ReturnType<typeof Bun.spawn> | null} */
let proc = null;
self.onmessage = async (event) => {
    if (event.data === 'stop') {
        proc?.kill();
        proc = null;
        self.close();
        return;
    }
    /** @type {WorkerStartMessage} */
    const payload = typeof event.data === 'string'
        ? { script: event.data, cwd: undefined, env: undefined }
        : event.data;
    const { script, cwd, env } = payload;
    // Resolve the script path relative to the project root before changing cwd
    const absoluteScript = Bun.resolveSync(script, process.cwd());
    proc?.kill();
    proc = Bun.spawn(['bun', absoluteScript], {
        cwd: cwd ? Bun.resolveSync(cwd, process.cwd()) : process.cwd(),
        env: env ? { ...process.env, ...env } : process.env,
        stdout: 'inherit',
        stderr: 'inherit'
    });
    await proc.exited;
};
