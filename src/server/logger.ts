type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'fatal'
type EffectiveLogLevel = LogLevel | 'silent'
type LogFormat = 'pretty' | 'json'
type LogFields = Record<string, unknown>

type LoggerOptions = {
    level?: EffectiveLogLevel
    format?: LogFormat
    colorize?: boolean
    now?: () => Date
    write?: (line: string, level: LogLevel) => void
}

export interface Logger {
    child(bindings: LogFields): Logger
    trace(message: string, fields?: LogFields): void
    debug(message: string, fields?: LogFields): void
    info(message: string, fields?: LogFields): void
    warn(message: string, fields?: LogFields): void
    error(message: string, fields?: LogFields): void
    fatal(message: string, fields?: LogFields): void
}

type LogEntry = {
    timestamp: string
    level: LogLevel
    message: string
    service?: string
    scope?: string
    err?: Record<string, unknown>
} & LogFields

const RESET = '\x1b[0m'
const DEFAULT_SERVICE = 'tachyon'

const LEVEL_SEVERITY: Record<EffectiveLogLevel, number> = {
    trace: 10,
    debug: 20,
    info: 30,
    warn: 40,
    error: 50,
    fatal: 60,
    silent: 70,
}

const LEVEL_COLORS: Record<LogLevel, string> = {
    trace: '\x1b[90m',
    debug: '\x1b[36m',
    info: '\x1b[32m',
    warn: '\x1b[33m',
    error: '\x1b[31m',
    fatal: '\x1b[35m',
}

const CONSOLE_LEVELS = {
    log: 'info',
    info: 'info',
    warn: 'warn',
    error: 'error',
    debug: 'debug',
    trace: 'trace',
} as const

function resolveLogLevel(value: string | undefined): EffectiveLogLevel {
    const normalized = value?.trim().toLowerCase()

    switch (normalized) {
        case 'trace':
        case 'debug':
        case 'info':
        case 'warn':
        case 'error':
        case 'fatal':
        case 'silent':
            return normalized
        case 'warning':
            return 'warn'
        default:
            return 'info'
    }
}

function resolveLogFormat(value: string | undefined): LogFormat {
    return value?.trim().toLowerCase() === 'json' ? 'json' : 'pretty'
}

function createWriteTarget() {
    return (line: string, level: LogLevel) => {
        const stream = level === 'warn' || level === 'error' || level === 'fatal'
            ? process.stderr
            : process.stdout

        stream.write(`${line}\n`)
    }
}

function serializeUnknown(value: unknown): unknown {
    if (value instanceof Error) return serializeError(value)
    if (typeof value === 'bigint') return value.toString()
    if (typeof value === 'function') return `[Function ${value.name || 'anonymous'}]`
    if (typeof value === 'symbol') return value.toString()
    if (value === undefined) return '[undefined]'
    return value
}

function serializeError(error: unknown): Record<string, unknown> {
    if (error instanceof Error) {
        const details: Record<string, unknown> = {
            name: error.name,
            message: error.message,
            stack: error.stack,
        }

        if ('cause' in error && error.cause !== undefined) {
            details.cause = serializeUnknown(error.cause)
        }

        return details
    }

    return { message: toInlineString(error) }
}

function safeJson(value: unknown): string {
    const seen = new WeakSet<object>()

    return JSON.stringify(value, (_key, currentValue) => {
        const normalized = serializeUnknown(currentValue)

        if (normalized && typeof normalized === 'object') {
            if (seen.has(normalized as object)) return '[Circular]'
            seen.add(normalized as object)
        }

        return normalized
    })
}

function needsQuoting(value: string): boolean {
    return /\s|=|"|,|\[|\]|\{|\}/.test(value)
}

function toInlineString(value: unknown): string {
    if (value === null) return 'null'
    if (value === undefined) return '[undefined]'
    if (typeof value === 'string') return needsQuoting(value) ? JSON.stringify(value) : value
    if (typeof value === 'number' || typeof value === 'boolean' || typeof value === 'bigint') return String(value)
    if (typeof value === 'function') return `[Function ${value.name || 'anonymous'}]`
    if (typeof value === 'symbol') return value.toString()
    if (value instanceof Error) return `${value.name}: ${value.message}`
    return safeJson(value)
}

function normalizeFields(fields: LogFields | undefined): LogFields {
    if (!fields) return {}

    const normalized: LogFields = {}

    for (const [key, value] of Object.entries(fields)) {
        if (value !== undefined) normalized[key] = value
    }

    return normalized
}

function normalizeEntry(
    level: LogLevel,
    message: string,
    bindings: LogFields,
    fields: LogFields,
    now: Date,
): LogEntry {
    const combined = { ...bindings, ...fields }
    const errorField = combined.err

    if ('err' in combined) delete combined.err

    return {
        timestamp: now.toISOString(),
        level,
        message,
        service: typeof combined.service === 'string' ? combined.service : DEFAULT_SERVICE,
        scope: typeof combined.scope === 'string' ? combined.scope : undefined,
        ...combined,
        err: errorField === undefined ? undefined : serializeError(errorField),
    }
}

function buildPrettyLine(entry: LogEntry, colorize: boolean): string {
    const { timestamp, level, message, service, scope, err, ...fields } = entry
    const upperLevel = level.toUpperCase().padEnd(5)
    const levelLabel = colorize ? `${LEVEL_COLORS[level]}${upperLevel}${RESET}` : upperLevel
    const namespace = scope ? `${service}/${scope}` : service
    const inlineFields = Object.entries(fields)
        .filter(([, value]) => value !== undefined)
        .map(([key, value]) => `${key}=${toInlineString(value)}`)
        .join(' ')

    const headline = `${timestamp} ${levelLabel} [${namespace}] ${message}`
    const body = inlineFields ? `${headline} ${inlineFields}` : headline

    if (!err) return body

    const summary = `${err.name ?? 'Error'}: ${err.message ?? 'Unknown error'}`
    const stack = typeof err.stack === 'string' && err.stack.length > 0 ? `\n${err.stack}` : ''
    return `${body}\n${summary}${stack}`
}

class TachyonLogger implements Logger {
    constructor(
        private readonly bindings: LogFields,
        private readonly options: Required<LoggerOptions>,
    ) {}

    child(bindings: LogFields): Logger {
        return new TachyonLogger(
            { ...this.bindings, ...normalizeFields(bindings) },
            this.options,
        )
    }

    trace(message: string, fields?: LogFields): void {
        this.log('trace', message, fields)
    }

    debug(message: string, fields?: LogFields): void {
        this.log('debug', message, fields)
    }

    info(message: string, fields?: LogFields): void {
        this.log('info', message, fields)
    }

    warn(message: string, fields?: LogFields): void {
        this.log('warn', message, fields)
    }

    error(message: string, fields?: LogFields): void {
        this.log('error', message, fields)
    }

    fatal(message: string, fields?: LogFields): void {
        this.log('fatal', message, fields)
    }

    private log(level: LogLevel, message: string, fields?: LogFields) {
        if (LEVEL_SEVERITY[level] < LEVEL_SEVERITY[this.options.level]) return

        const entry = normalizeEntry(
            level,
            message,
            this.bindings,
            normalizeFields(fields),
            this.options.now(),
        )

        const line = this.options.format === 'json'
            ? safeJson(entry)
            : buildPrettyLine(entry, this.options.colorize)

        this.options.write(line, level)
    }
}

export function createLogger(bindings: LogFields = {}, options: LoggerOptions = {}): Logger {
    const format = options.format ?? resolveLogFormat(process.env.TACHYON_LOG_FORMAT ?? process.env.LOG_FORMAT)

    return new TachyonLogger(
        normalizeFields(bindings),
        {
            level: options.level ?? resolveLogLevel(process.env.TACHYON_LOG_LEVEL ?? process.env.LOG_LEVEL),
            format,
            colorize: options.colorize ?? (process.stdout.isTTY && format === 'pretty'),
            now: options.now ?? (() => new Date()),
            write: options.write ?? createWriteTarget(),
        },
    )
}

function normalizeConsoleArgs(args: unknown[]): { message: string, fields: LogFields } {
    if (args.length === 0) return { message: '', fields: {} }

    const [first, ...rest] = args

    if (typeof first === 'string') {
        return {
            message: first,
            fields: rest.length > 0 ? { args: rest } : {},
        }
    }

    if (first instanceof Error) {
        return {
            message: first.message,
            fields: {
                err: first,
                args: rest.length > 0 ? rest : undefined,
            },
        }
    }

    return {
        message: toInlineString(first),
        fields: rest.length > 0 ? { args: rest } : {},
    }
}

let consoleInstalled = false

const logger = createLogger({ service: DEFAULT_SERVICE })

export function installConsoleLogger(baseLogger: Logger = logger) {
    if (consoleInstalled) return
    consoleInstalled = true

    for (const [method, level] of Object.entries(CONSOLE_LEVELS)) {
        console[method as keyof typeof CONSOLE_LEVELS] = (...args: unknown[]) => {
            const { message, fields } = normalizeConsoleArgs(args)
            baseLogger[level](message, fields)
        }
    }
}

export default logger
