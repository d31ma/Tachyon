// @ts-check

class YonJsRunner {
    static RESPONSE_START = '\x1fTACHYON_RESPONSE\x1e';
    static RESPONSE_END = '\x1eTACHYON_RESPONSE\x1f';

    /**
     * @param {unknown} exported
     * @returns {(request: unknown) => unknown | Promise<unknown>}
     */
    static resolveHandler(exported) {
        if (typeof exported === 'function') {
            if (/^class\s/.test(Function.prototype.toString.call(exported))) {
                const HandlerClass = /** @type {new () => { handler?: unknown }} */ (exported);
                const instance = new HandlerClass();
                if (typeof instance.handler !== 'function')
                    throw new Error('Default class export must define handler(request)');
                return instance.handler.bind(instance);
            }
            return /** @type {(request: unknown) => unknown | Promise<unknown>} */ (exported);
        }
        const record = /** @type {{ handler?: unknown } | null} */ (exported && typeof exported === 'object' ? exported : null);
        if (record && typeof record.handler === 'function') {
            return record.handler.bind(record);
        }
        throw new Error('Route module must export handler(request), a default function, or a default class with handler(request)');
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
        const module = await import(`${handlerPath}?t=${Date.now()}`);
        const handler = module.handler
            ? YonJsRunner.resolveHandler(module)
            : YonJsRunner.resolveHandler(module.default);
        const result = await handler(request);
        YonJsRunner.writeResponseFrame(YonJsRunner.serialize(result));
    }
}

YonJsRunner.run().catch((error) => {
    const message = error instanceof Error ? error.stack || error.message : String(error);
    Bun.stderr.write(message);
    process.exit(1);
});
