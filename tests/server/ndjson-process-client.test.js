// @ts-check
import { expect, test } from 'bun:test';
import { EventEmitter } from 'node:events';
import { mkdir, mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import NdjsonProcessClient from '../../src/vendor/shared/ndjson-process-client.mjs';

class FakePipe extends EventEmitter {
    /** @type {Error | null} */
    writeError = null;
    writableEnded = false;

    setEncoding() { }
    ref() { }
    unref() { }

    /** @param {string} _chunk @param {(error?: Error | null) => void} [callback] */
    write(_chunk, callback) {
        queueMicrotask(() => callback?.(this.writeError));
        return !this.writeError;
    }

    end() {
        this.writableEnded = true;
    }
}

class FakeProcess extends EventEmitter {
    stdin = new FakePipe();
    stdout = new FakePipe();
    exitCode = null;
    killed = false;

    constructor() {
        super();
        const end = this.stdin.end.bind(this.stdin);
        this.stdin.end = () => {
            end();
            queueMicrotask(() => {
                if (this.exitCode === null) {
                    this.exitCode = 0;
                    this.emit('exit', 0, null);
                }
            });
        };
    }

    ref() { }
    unref() { }

    kill() {
        this.killed = true;
        this.exitCode = 1;
        this.emit('exit', 1, null);
    }
}

test('native vendor binaries take precedence over node_modules command shims', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-vendor-command-'));
    const packageBin = path.join(root, 'node_modules', '.bin');
    const nativeBin = path.join(root, '.local', 'bin');
    const executable = process.platform === 'win32' ? 'fylo.exe' : 'fylo';
    await mkdir(packageBin, { recursive: true });
    await mkdir(nativeBin, { recursive: true });
    await writeFile(path.join(packageBin, executable), 'package shim');
    await writeFile(path.join(nativeBin, executable), 'native binary');

    try {
        expect(NdjsonProcessClient.resolveCommand('fylo', undefined, {
            PATH: `${packageBin}${path.delimiter}${nativeBin}`,
        })).toBe(path.join(nativeBin, executable));
    }
    finally {
        await rm(root, { recursive: true, force: true });
    }
});

test('native subprocesses prioritize sibling helper binaries', () => {
    const command = path.resolve('/native/bin/fylo');
    const packageBin = path.resolve('/workspace/node_modules/.bin');
    const env = NdjsonProcessClient.commandEnvironment(command, {
        PATH: `${packageBin}${path.delimiter}${path.dirname(command)}`,
    });
    expect(env.PATH?.split(path.delimiter)).toEqual([
        path.dirname(command),
        packageBin,
    ]);
});

test('malformed NDJSON rejects the request and a later request uses a fresh process', async () => {
    const first = new FakeProcess();
    const second = new FakeProcess();
    const processes = [first, second];
    const client = new NdjsonProcessClient({
        name: 'fylo',
        command: 'fylo',
        args: ['exec', '--loop'],
        spawnProcess: () => processes.shift(),
    });

    const malformed = client.request({ op: 'inspectCollection' });
    first.stdout.emit('data', '{"ok":tru\n');
    await expect(malformed).rejects.toThrow('fylo emitted invalid NDJSON');
    expect(first.killed).toBe(true);

    const recovered = client.request({ op: 'inspectCollection' });
    second.stdout.emit('data', '{"ok":true,"result":{"exists":true}}\n');
    await expect(recovered).resolves.toEqual({ ok: true, result: { exists: true } });
    expect(client._proc).toBe(second);
    await client.close();
});

test('stdin EPIPE rejects every pending request without an unhandled stream error', async () => {
    const process = new FakeProcess();
    process.stdin.writeError = Object.assign(new Error('broken pipe'), { code: 'EPIPE' });
    const client = new NdjsonProcessClient({
        name: 'chex',
        command: 'chex',
        args: ['exec', '--loop'],
        spawnProcess: () => process,
    });

    const first = client.request({ op: 'validate' });
    const second = client.request({ op: 'validate' });
    const outcomes = await Promise.allSettled([first, second]);
    expect(outcomes.every((outcome) => outcome.status === 'rejected')).toBe(true);
    for (const outcome of outcomes) {
        if (outcome.status === 'rejected') {
            expect(outcome.reason).toBeInstanceOf(Error);
            expect(outcome.reason.message).toBe('chex stdin failed: broken pipe');
        }
    }
    expect(process.killed).toBe(true);
    await client.close();
});

test('an exited subprocess rejects pending work and is restarted on the next request', async () => {
    const first = new FakeProcess();
    const second = new FakeProcess();
    const processes = [first, second];
    const client = new NdjsonProcessClient({
        name: 'fylo',
        command: 'fylo',
        args: ['exec', '--loop'],
        spawnProcess: () => processes.shift(),
    });

    const interrupted = client.request({ op: 'findDocs' });
    first.exitCode = 9;
    first.emit('exit', 9, null);
    await expect(interrupted).rejects.toThrow('fylo process exited with code 9');

    const recovered = client.request({ op: 'findDocs' });
    second.stdout.emit('data', '{"ok":true,"result":{}}\n');
    await expect(recovered).resolves.toEqual({ ok: true, result: {} });
    await client.close();
});
