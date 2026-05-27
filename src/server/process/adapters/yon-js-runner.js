// @ts-check

class YonJsRunner {
    static RESPONSE_START = '\x1fTACHYON_RESPONSE\x1e';
    static RESPONSE_END = '\x1eTACHYON_RESPONSE\x1f';

    /**
     * Resolves the Handler class from a module's exports.
     * Looks for a named `Handler` export first, then checks the default export.
     * @param {unknown} module
     * @returns {{ new?(): unknown } & Record<string, unknown>}
     */
    static resolveHandlerClass(module) {
        const mod = /** @type {Record<string, unknown>} */ (module);
        // Named export: export class Handler { ... }
        if (mod.Handler && typeof mod.Handler === 'function')
            return /** @type {any} */ (mod.Handler);
        // Default export: export default class Handler { ... }
        if (mod.default && typeof mod.default === 'function') {
            const defaultExport = /** @type {any} */ (mod.default);
            if (/^class\s/.test(Function.prototype.toString.call(defaultExport)))
                return defaultExport;
        }
        throw new Error('Route module must export a class named Handler with static HTTP method handlers');
    }

    /**
     * @param {unknown} value
     * @returns {string}
     */
    static serialize(value) {
        if (value === undefined)
            return '';
        if (typeof value === 'string')
            return value;
        return JSON.stringify(value);
    }

    /**
     * @param {unknown} value
     * @returns {value is ReadableStream<unknown>}
     */
    static isReadableStream(value) {
        return !!value
            && typeof value === 'object'
            && typeof /** @type {{ getReader?: unknown }} */ (value).getReader === 'function';
    }

    /**
     * @param {unknown} value
     * @returns {value is AsyncIterable<unknown>}
     */
    static isAsyncIterable(value) {
        return !!value
            && typeof value === 'object'
            && Symbol.asyncIterator in /** @type {Record<symbol, unknown>} */ (value);
    }

    /** @param {unknown} chunk */
    static writeChunk(chunk) {
        if (chunk === undefined || chunk === null)
            return;
        if (chunk instanceof Uint8Array) {
            Bun.stdout.write(chunk);
            return;
        }
        Bun.stdout.write(typeof chunk === 'string' ? chunk : YonJsRunner.serialize(chunk));
    }

    /**
     * @param {ReadableStream<unknown>} stream
     */
    static async writeReadableStream(stream) {
        const reader = stream.getReader();
        try {
            while (true) {
                const { value, done } = await reader.read();
                if (done)
                    break;
                YonJsRunner.writeChunk(value);
            }
        }
        finally {
            reader.releaseLock();
        }
    }

    /**
     * @param {AsyncIterable<unknown>} iterable
     */
    static async writeAsyncIterable(iterable) {
        for await (const chunk of iterable)
            YonJsRunner.writeChunk(chunk);
    }

    static installConsoleSideband() {
        /**
         * @param {string} level
         * @param {unknown[]} args
         */
        const write = (level, args) => {
            const line = args.map((arg) => {
                if (typeof arg === 'string')
                    return arg;
                try {
                    return JSON.stringify(arg);
                }
                catch {
                    return String(arg);
                }
            }).join(' ');
            Bun.stderr.write(`[${level}] ${line}\n`);
        };
        console.log = (...args) => write('log', args);
        console.info = (...args) => write('info', args);
        console.debug = (...args) => write('debug', args);
        console.warn = (...args) => write('warn', args);
        console.error = (...args) => write('error', args);
    }

    /** @param {string} body */
    static writeResponseFrame(body) {
        Bun.stdout.write(`${YonJsRunner.RESPONSE_START}${body}${YonJsRunner.RESPONSE_END}`);
    }

    static async run() {
        const handlerPath = process.argv[2];
        if (!handlerPath)
            throw new Error('Missing handler path');
        YonJsRunner.installConsoleSideband();
        const request = await Bun.stdin.json();
        const method = request?.method;
        if (!method)
            throw new Error('Missing HTTP method in request payload');
        const module = await import(`${handlerPath}?t=${Date.now()}`);
        const Handler = YonJsRunner.resolveHandlerClass(module);
        const dispatch = /** @type {Function | undefined} */ (Handler[method]);
        if (typeof dispatch !== 'function')
            throw new Error(`Handler class does not implement static ${method}()`);
        const result = await dispatch(request);
        if (request?.headers?.accept === 'text/event-stream') {
            if (YonJsRunner.isReadableStream(result)) {
                await YonJsRunner.writeReadableStream(result);
                return;
            }
            if (YonJsRunner.isAsyncIterable(result)) {
                await YonJsRunner.writeAsyncIterable(result);
                return;
            }
        }
        YonJsRunner.writeResponseFrame(YonJsRunner.serialize(result));
    }
}

YonJsRunner.run().catch((error) => {
    const message = error instanceof Error ? error.stack || error.message : String(error);
    Bun.stderr.write(message);
    process.exit(1);
});
