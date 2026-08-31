// Shared process boundary for blackbox acceptance of a selected release binary.
import { spawn } from 'node:child_process';

/**
 * @typedef {(path: string, options?: RequestInit) => Promise<Response>} ProbeRequest
 * @param {string} binary
 * @param {string[]} args
 * @param {(request: ProbeRequest) => Promise<void>} probe
 */
export async function withServer(binary, args, probe) {
  const child = spawn(binary, args, { stdio: ['ignore', 'pipe', 'pipe'] });
  let closed = false;
  /** @type {Promise<void>} */
  const exited = new Promise(resolve => child.once('close', () => { closed = true; resolve(); }));
  let logs = '';
  let pending = '';
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let readinessTimer;
  /** @type {Promise<string>} */
  const ready = new Promise((resolve, reject) => {
    readinessTimer = setTimeout(() => reject(new Error(`Server readiness timed out: ${logs}`)), 30_000);
    child.once('error', reject);
    child.once('close', code => reject(new Error(`Server exited before readiness (${code}): ${logs}`)));
    child.stderr.on('data', chunk => { logs = (logs + chunk).slice(-32768); });
    child.stdout.on('data', chunk => {
      logs = (logs + chunk).slice(-32768);
      pending = (pending + chunk).slice(-32768);
      const match = pending.match(/ready at (http:\/\/127\.0\.0\.1:\d+)\//);
      if (match) resolve(match[1]);
    });
  });
  try {
    const origin = await ready;
    clearTimeout(readinessTimer);
    /** @type {ProbeRequest} */
    const request = (path, options = {}) => fetch(`${origin}${path}`, {
      ...options, signal: AbortSignal.timeout(10_000),
    });
    return await probe(request);
  } finally {
    clearTimeout(readinessTimer);
    if (!closed) child.kill('SIGTERM');
    let killTimer;
    let deadline;
    try {
      killTimer = setTimeout(() => { if (!closed) child.kill('SIGKILL'); }, 5000);
      await Promise.race([
        exited,
        new Promise((_, reject) => {
          deadline = setTimeout(() => reject(new Error('Server failed to exit after forced termination')), 10_000);
        }),
      ]);
    } finally {
      clearTimeout(killTimer);
      clearTimeout(deadline);
    }
  }
}
