// @ts-check

class YonJsRunner {
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

    static async run() {
        const handlerPath = process.argv[2];
        if (!handlerPath)
            throw new Error('Missing handler path');
        const request = await Bun.stdin.json();
        const module = await import(`${handlerPath}?t=${Date.now()}`);
        const handler = module.handler
            ? YonJsRunner.resolveHandler(module)
            : YonJsRunner.resolveHandler(module.default);
        const result = await handler(request);
        Bun.stdout.write(YonJsRunner.serialize(result));
    }
}

YonJsRunner.run().catch((error) => {
    const message = error instanceof Error ? error.stack || error.message : String(error);
    Bun.stderr.write(message);
    process.exit(1);
});
