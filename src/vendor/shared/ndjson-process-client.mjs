// @ts-nocheck
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';

/**
 * Owns one restartable subprocess that exchanges one JSON object per line.
 * Protocol failures are terminal for the current process because continuing
 * after a malformed response could associate later responses with wrong calls.
 */
export default class NdjsonProcessClient {
    static _instances = new Set();

    /**
     * @param {{
     *   name: string,
     *   command: string,
     *   args: string[],
     *   spawnProcess?: typeof spawn,
     * }} options
     */
    constructor(options) {
        this._name = options.name;
        this._command = options.command;
        this._args = options.args;
        this._spawnProcess = options.spawnProcess ?? spawn;
        this._proc = null;
        this._queue = [];
        this._buffer = '';
        this._closed = false;
        this._needsRestart = false;
        this._spawn();
        NdjsonProcessClient._instances.add(this);
    }

    /** Close every live client owned by the current process. */
    static async closeAll() {
        const clients = [...NdjsonProcessClient._instances];
        await Promise.allSettled(clients.map((client) => client.close()));
    }

    /** Prefer installed native binaries over package-manager command shims. */
    static resolveCommand(command, explicit, env = process.env) {
        if (explicit) return explicit;
        const executableNames = process.platform === 'win32'
            ? [`${command}.exe`, command]
            : [command];
        const candidates = [];
        for (const directory of String(env.PATH ?? '').split(path.delimiter)) {
            if (!directory) continue;
            for (const executable of executableNames) {
                const candidate = path.join(directory, executable);
                if (existsSync(candidate)) candidates.push(candidate);
            }
        }
        const native = candidates.find((candidate) => {
            const normalized = candidate.replaceAll('\\', '/').toLowerCase();
            return !normalized.includes('/node_modules/.bin/');
        });
        return native ?? command;
    }

    /** Keep sibling native helpers ahead of package-manager shims. */
    static commandEnvironment(command, env = process.env) {
        if (!path.isAbsolute(command)) return env;
        const commandDirectory = path.dirname(command);
        const directories = String(env.PATH ?? '')
            .split(path.delimiter)
            .filter((directory) => directory && path.resolve(directory) !== commandDirectory);
        return {
            ...env,
            PATH: [commandDirectory, ...directories].join(path.delimiter),
        };
    }

    _spawn() {
        const proc = this._spawnProcess(this._command, this._args, {
            stdio: ['pipe', 'pipe', 'inherit'],
            env: NdjsonProcessClient.commandEnvironment(this._command),
        });
        if (!proc?.stdin || !proc?.stdout) {
            throw new Error(`${this._name} process did not expose piped stdin/stdout`);
        }
        this._proc = proc;
        this._buffer = '';
        this._needsRestart = false;
        proc.stdout.setEncoding('utf8');
        proc.stdout.on('data', (chunk) => {
            if (this._proc === proc) this._onData(String(chunk));
        });
        proc.stdout.on('end', () => {
            if (this._proc === proc && this._queue.length > 0 && this._buffer.trim()) {
                this._fail(new Error(`${this._name} emitted truncated NDJSON before stdout closed`));
            }
        });
        proc.stdin.on('error', (error) => {
            if (this._proc === proc) this._fail(this._ioError('stdin', error));
        });
        proc.on('error', (error) => {
            if (this._proc === proc) this._fail(this._ioError('process', error));
        });
        proc.on('exit', (code, signal) => {
            if (this._proc !== proc) return;
            this._needsRestart = !this._closed;
            const reason = code === null
                ? `${this._name} process exited${signal ? ` from signal ${signal}` : ''}`
                : `${this._name} process exited with code ${code}`;
            this._rejectAll(new Error(reason));
        });
    }

    /** @param {'stdin' | 'process'} source @param {unknown} error */
    _ioError(source, error) {
        const detail = error instanceof Error ? error.message : String(error);
        return new Error(`${this._name} ${source} failed: ${detail}`, { cause: error });
    }

    /** @param {string} chunk */
    _onData(chunk) {
        this._buffer += chunk;
        let newline;
        while ((newline = this._buffer.indexOf('\n')) !== -1) {
            const line = this._buffer.slice(0, newline).trim();
            this._buffer = this._buffer.slice(newline + 1);
            if (!line) continue;
            let response;
            try {
                response = JSON.parse(line);
            }
            catch (error) {
                this._fail(new Error(`${this._name} emitted invalid NDJSON`, { cause: error }));
                return;
            }
            const pending = this._queue.shift();
            if (pending) pending.resolve(response);
        }
    }

    /** @param {Error} error */
    _fail(error) {
        this._needsRestart = !this._closed;
        this._rejectAll(error);
        const proc = this._proc;
        if (proc && proc.exitCode === null) {
            try { proc.kill(); } catch { /* process already exited */ }
        }
    }

    /** @param {Error} error */
    _rejectAll(error) {
        for (const pending of this._queue.splice(0)) pending.reject(error);
    }

    _activeProcess() {
        if (this._closed) throw new Error(`${this._name} client is closed`);
        if (!this._proc || this._needsRestart || this._proc.exitCode !== null) this._spawn();
        return this._proc;
    }

    /** Send one request and resolve with its ordered NDJSON response. */
    request(operation) {
        let proc;
        try {
            proc = this._activeProcess();
        }
        catch (error) {
            return Promise.reject(error);
        }
        return new Promise((resolve, reject) => {
            this._queue.push({ resolve, reject });
            try {
                proc.stdin.write(`${JSON.stringify(operation)}\n`, (error) => {
                    if (error && this._proc === proc) this._fail(this._ioError('stdin', error));
                });
            }
            catch (error) {
                this._fail(this._ioError('stdin', error));
            }
        });
    }

    /** Close stdin and wait for the active subprocess to exit. */
    close() {
        if (this._closed) return Promise.resolve();
        this._closed = true;
        NdjsonProcessClient._instances.delete(this);
        this._needsRestart = false;
        this._rejectAll(new Error(`${this._name} client closed before receiving a response`));
        const proc = this._proc;
        if (!proc || proc.exitCode !== null) return Promise.resolve();
        return new Promise((resolve) => {
            proc.once('exit', resolve);
            try {
                proc.stdin.end();
            }
            catch {
                try { proc.kill(); } catch { /* process already exited */ }
                resolve();
            }
        });
    }
}
