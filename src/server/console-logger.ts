/** ANSI reset escape code */
const reset = '\x1b[0m'

/** Returns a formatted UTC timestamp string */
const formatDate = (): string => new Date().toISOString().replace('T', ' ').replace('Z', '')

const LOG_LEVEL_COLORS = {
    INFO:  '\x1b[32m',
    ERROR: '\x1b[31m',
    DEBUG: '\x1b[36m',
    WARN:  '\x1b[33m',
    TRACE: '\x1b[35m',
} as const

type LogLevel = keyof typeof LOG_LEVEL_COLORS

/**
 * Creates a leveled console logger that prepends a timestamp and log level prefix.
 * The last argument is treated as the context identifier (e.g. process PID).
 * @param level - The log level label
 * @returns A console-compatible logger function
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function createLeveledLogger(level: LogLevel): (...args: any[]) => void {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return (...args: any[]) => {
        const context = args.length > 1 ? args.pop() : process.pid
        const color = process.stdout.isTTY ? LOG_LEVEL_COLORS[level] : ''
        const resetCode = process.stdout.isTTY ? reset : ''
        const prefix = `[${formatDate()}]${color} ${level}${resetCode} (${context})`
        console.log(prefix, ...args)
    }
}

console.info  = createLeveledLogger('INFO')
console.error = createLeveledLogger('ERROR')
console.debug = createLeveledLogger('DEBUG')
console.warn  = createLeveledLogger('WARN')
console.trace = createLeveledLogger('TRACE')
