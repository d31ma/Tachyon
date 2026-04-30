// @ts-check
/**
 * @typedef {'trace' | 'debug' | 'info' | 'warn' | 'error' | 'fatal' | 'silent'} LogLevel
 * @typedef {'pretty' | 'json'} LogFormat
 * @typedef {Record<string, unknown>} LoggerFields
 * @typedef {{ name?: string, message?: string, stack?: string, cause?: unknown }} SerializedError
 * @typedef {{ timestamp: string, level: LogLevel, message: string, service: string, scope?: string, err?: SerializedError } & LoggerFields} LogEntry
 * @typedef {{ level: LogLevel, format: LogFormat, colorize: boolean, now: () => Date, write: (line: string, level: LogLevel) => void }} LoggerOptions
 */
const RESET = '\x1b[0m';
const DEFAULT_SERVICE = 'tachyon';
/** @type {Record<LogLevel, number>} */
const LEVEL_SEVERITY = {
    trace: 10,
    debug: 20,
    info: 30,
    warn: 40,
    error: 50,
    fatal: 60,
    silent: 70,
};
/** @type {Record<Exclude<LogLevel, 'silent'>, string>} */
const LEVEL_COLORS = {
    trace: '\x1b[90m',
    debug: '\x1b[36m',
    info: '\x1b[32m',
    warn: '\x1b[33m',
    error: '\x1b[31m',
    fatal: '\x1b[35m',
};
/** @type {Record<'log' | 'info' | 'warn' | 'error' | 'debug' | 'trace', Exclude<LogLevel, 'silent'>>} */
const CONSOLE_LEVELS = {
    log: 'info',
    info: 'info',
    warn: 'warn',
    error: 'error',
    debug: 'debug',
    trace: 'trace',
};
/** @param {string | undefined | null} value @returns {LogLevel} */
function resolveLogLevel(value) {
    const normalized = value?.trim().toLowerCase();
    switch (normalized) {
        case 'trace':
        case 'debug':
        case 'info':
        case 'warn':
        case 'error':
        case 'fatal':
        case 'silent':
            return normalized;
        case 'warning':
            return 'warn';
        default:
            return 'info';
    }
}
/** @param {string | undefined | null} value @returns {LogFormat} */
function resolveLogFormat(value) {
    return value?.trim().toLowerCase() === 'json' ? 'json' : 'pretty';
}
function createWriteTarget() {
    /** @param {string} line @param {LogLevel} level */
    return (line, level) => {
        const stream = level === 'warn' || level === 'error' || level === 'fatal'
            ? process.stderr
            : process.stdout;
        stream.write(`${line}\n`);
    };
}
/** @param {unknown} value @returns {unknown} */
function serializeUnknown(value) {
    if (value instanceof Error)
        return serializeError(value);
    if (typeof value === 'bigint')
        return value.toString();
    if (typeof value === 'function')
        return `[Function ${value.name || 'anonymous'}]`;
    if (typeof value === 'symbol')
        return value.toString();
    if (value === undefined)
        return '[undefined]';
    return value;
}
/** @param {unknown} error @returns {SerializedError} */
function serializeError(error) {
    if (error instanceof Error) {
        /** @type {SerializedError} */
        const details = {
            name: error.name,
            message: error.message,
            stack: error.stack,
        };
        if ('cause' in error && error.cause !== undefined) {
            details.cause = serializeUnknown(error.cause);
        }
        return details;
    }
    return { message: toInlineString(error) };
}
/** @param {unknown} value @returns {string} */
function safeJson(value) {
    const seen = new WeakSet();
    return JSON.stringify(value, (_key, currentValue) => {
        const normalized = serializeUnknown(currentValue);
        if (normalized && typeof normalized === 'object') {
            if (seen.has(normalized))
                return '[Circular]';
            seen.add(normalized);
        }
        return normalized;
    });
}
/** @param {string} value */
function needsQuoting(value) {
    return /\s|=|"|,|\[|\]|\{|\}/.test(value);
}
/** @param {unknown} value @returns {string} */
function toInlineString(value) {
    if (value === null)
        return 'null';
    if (value === undefined)
        return '[undefined]';
    if (typeof value === 'string')
        return needsQuoting(value) ? JSON.stringify(value) : value;
    if (typeof value === 'number' || typeof value === 'boolean' || typeof value === 'bigint')
        return String(value);
    if (typeof value === 'function')
        return `[Function ${value.name || 'anonymous'}]`;
    if (typeof value === 'symbol')
        return value.toString();
    if (value instanceof Error)
        return `${value.name}: ${value.message}`;
    return safeJson(value);
}
/** @param {LoggerFields | undefined} fields @returns {LoggerFields} */
function normalizeFields(fields) {
    if (!fields)
        return {};
    /** @type {LoggerFields} */
    const normalized = {};
    for (const [key, value] of Object.entries(fields)) {
        if (value !== undefined)
            normalized[key] = value;
    }
    return normalized;
}
/**
 * @param {LogLevel} level
 * @param {string} message
 * @param {LoggerFields} bindings
 * @param {LoggerFields} fields
 * @param {Date} now
 * @returns {LogEntry}
 */
function normalizeEntry(level, message, bindings, fields, now) {
    const combined = { ...bindings, ...fields };
    const errorField = combined.err;
    if ('err' in combined)
        delete combined.err;
    return {
        timestamp: now.toISOString(),
        level,
        message,
        service: typeof combined.service === 'string' ? combined.service : DEFAULT_SERVICE,
        scope: typeof combined.scope === 'string' ? combined.scope : undefined,
        ...combined,
        err: errorField === undefined ? undefined : serializeError(errorField),
    };
}
/** @param {LogEntry} entry @param {boolean} colorize */
function buildPrettyLine(entry, colorize) {
    const { timestamp, level, message, service, scope, err, ...fields } = entry;
    const upperLevel = level.toUpperCase().padEnd(5);
    const color = level === 'silent' ? '' : LEVEL_COLORS[level];
    const levelLabel = colorize && color ? `${color}${upperLevel}${RESET}` : upperLevel;
    const namespace = scope ? `${service}/${scope}` : service;
    const inlineFields = Object.entries(fields)
        .filter(([, value]) => value !== undefined)
        .map(([key, value]) => `${key}=${toInlineString(value)}`)
        .join(' ');
    const headline = `${timestamp} ${levelLabel} [${namespace}] ${message}`;
    const body = inlineFields ? `${headline} ${inlineFields}` : headline;
    if (!err)
        return body;
    const summary = `${err.name ?? 'Error'}: ${err.message ?? 'Unknown error'}`;
    const stack = typeof err.stack === 'string' && err.stack.length > 0 ? `\n${err.stack}` : '';
    return `${body}\n${summary}${stack}`;
}
class TachyonLogger {
    /** @type {LoggerFields} */ bindings;
    /** @type {LoggerOptions} */ options;
    /** @param {LoggerFields} bindings @param {LoggerOptions} options */
    constructor(bindings, options) {
        this.bindings = bindings;
        this.options = options;
    }
    /** @param {LoggerFields} bindings */
    child(bindings) {
        return new TachyonLogger({ ...this.bindings, ...normalizeFields(bindings) }, this.options);
    }
    /** @param {string} message @param {LoggerFields} [fields] */
    trace(message, fields) {
        this.log('trace', message, fields);
    }
    /** @param {string} message @param {LoggerFields} [fields] */
    debug(message, fields) {
        this.log('debug', message, fields);
    }
    /** @param {string} message @param {LoggerFields} [fields] */
    info(message, fields) {
        this.log('info', message, fields);
    }
    /** @param {string} message @param {LoggerFields} [fields] */
    warn(message, fields) {
        this.log('warn', message, fields);
    }
    /** @param {string} message @param {LoggerFields} [fields] */
    error(message, fields) {
        this.log('error', message, fields);
    }
    /** @param {string} message @param {LoggerFields} [fields] */
    fatal(message, fields) {
        this.log('fatal', message, fields);
    }
    /** @param {LogLevel} level @param {string} message @param {LoggerFields} [fields] */
    log(level, message, fields) {
        if (LEVEL_SEVERITY[level] < LEVEL_SEVERITY[this.options.level])
            return;
        const entry = normalizeEntry(level, message, this.bindings, normalizeFields(fields), this.options.now());
        const line = this.options.format === 'json'
            ? safeJson(entry)
            : buildPrettyLine(entry, this.options.colorize);
        this.options.write(line, level);
    }
}
/**
 * @param {LoggerFields} [bindings]
 * @param {Partial<LoggerOptions>} [options]
 */
export function createLogger(bindings = {}, options = {}) {
    const format = options.format ?? resolveLogFormat(process.env.YON_LOG_FORMAT);
    return new TachyonLogger(normalizeFields(bindings), {
        level: options.level ?? resolveLogLevel(process.env.YON_LOG_LEVEL),
        format,
        colorize: options.colorize ?? (process.stdout.isTTY && format === 'pretty'),
        now: options.now ?? (() => new Date()),
        write: options.write ?? createWriteTarget(),
    });
}
/** @param {unknown[]} args */
function normalizeConsoleArgs(args) {
    if (args.length === 0)
        return { message: '', fields: {} };
    const [first, ...rest] = args;
    if (typeof first === 'string') {
        return {
            message: first,
            fields: rest.length > 0 ? { args: rest } : {},
        };
    }
    if (first instanceof Error) {
        return {
            message: first.message,
            fields: {
                err: first,
                args: rest.length > 0 ? rest : undefined,
            },
        };
    }
    return {
        message: toInlineString(first),
        fields: rest.length > 0 ? { args: rest } : {},
    };
}
let consoleInstalled = false;
const logger = createLogger({ service: DEFAULT_SERVICE });
/** @param {TachyonLogger} [baseLogger] */
export function installConsoleLogger(baseLogger = logger) {
    if (consoleInstalled)
        return;
    consoleInstalled = true;
    for (const [method, level] of Object.entries(CONSOLE_LEVELS)) {
        /** @type {(message?: unknown, ...optionalParams: unknown[]) => void} */
        const consoleMethod = (...args) => {
            const { message, fields } = normalizeConsoleArgs(args);
            baseLogger[level](message, fields);
        };
        /** @type {any} */ (console)[method] = consoleMethod;
    }
}
export default logger;
