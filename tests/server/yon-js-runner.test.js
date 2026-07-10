// @ts-check
import { expect, test } from 'bun:test';
import { mkdtemp, rm } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import { pathToFileURL } from 'url';

const RUNNER = path.join(import.meta.dir, '../../src/server/process/adapters/yon-js-runner.js');
const PROCESS_CLIENT = path.join(import.meta.dir, '../../src/vendor/shared/ndjson-process-client.mjs');

test('JavaScript handlers close persistent NDJSON clients before the runner exits', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-yon-js-runner-'));
    const helper = path.join(root, 'ndjson-helper.js');
    const handler = path.join(root, 'yon.js');
    await Bun.write(helper, `
let buffer = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk) => {
  buffer += chunk;
  let newline;
  while ((newline = buffer.indexOf('\\n')) !== -1) {
    buffer = buffer.slice(newline + 1);
    process.stdout.write('{"ok":true,"result":{"ready":true}}\\n');
  }
});
setTimeout(() => process.exit(0), 5000).unref();
`);
    await Bun.write(handler, `
import NdjsonProcessClient from ${JSON.stringify(pathToFileURL(PROCESS_CLIENT).href)};

export class Handler {
  static async GET() {
    const client = new NdjsonProcessClient({
      name: 'fixture',
      command: process.execPath,
      args: [${JSON.stringify(helper)}],
    });
    await client.request({ op: 'ready' });
    return { ok: true };
  }
}
`);

    const proc = Bun.spawn([process.execPath, RUNNER, handler], {
        stdin: 'pipe',
        stdout: 'pipe',
        stderr: 'pipe',
    });
    proc.stdin.write(JSON.stringify({ method: 'GET' }));
    proc.stdin.end();

    try {
        const exitedPromptly = await Promise.race([
            proc.exited.then(() => true),
            Bun.sleep(1_500).then(() => false),
        ]);
        expect(exitedPromptly).toBe(true);
        expect(await new Response(proc.stdout).text()).toContain('{"ok":true}');
        expect(await new Response(proc.stderr).text()).toBe('');
    }
    finally {
        if (proc.exitCode === null) proc.kill();
        await proc.exited.catch(() => { });
        await rm(root, { recursive: true, force: true });
    }
});
