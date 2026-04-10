import { expect, test } from 'bun:test'
import { createLogger } from '../../src/server/logger.js'

const fixedNow = () => new Date('2026-04-10T12:00:00.000Z')

test('pretty logger includes level, namespace, message, and structured fields', () => {
    const lines: string[] = []
    const logger = createLogger(
        { service: 'tachyon', scope: 'bundle' },
        {
            level: 'debug',
            format: 'pretty',
            colorize: false,
            now: fixedNow,
            write: (line) => lines.push(line),
        },
    )

    logger.info('Bundle completed', {
        routeCount: 7,
        distPath: '/tmp/dist',
        durationMs: 18,
    })

    expect(lines).toHaveLength(1)
    expect(lines[0]).toContain('2026-04-10T12:00:00.000Z')
    expect(lines[0]).toContain('INFO ')
    expect(lines[0]).toContain('[tachyon/bundle]')
    expect(lines[0]).toContain('Bundle completed')
    expect(lines[0]).toContain('routeCount=7')
    expect(lines[0]).toContain('distPath=/tmp/dist')
    expect(lines[0]).toContain('durationMs=18')
    expect(lines[0]).not.toContain('pid=')
})

test('logger respects configured level filtering', () => {
    const lines: string[] = []
    const logger = createLogger(
        { service: 'tachyon' },
        {
            level: 'warn',
            format: 'pretty',
            colorize: false,
            now: fixedNow,
            write: (line) => lines.push(line),
        },
    )

    logger.info('Suppressed')
    logger.warn('Visible warning')

    expect(lines).toHaveLength(1)
    expect(lines[0]).toContain('Visible warning')
    expect(lines[0]).not.toContain('Suppressed')
})

test('json logger serializes child bindings and errors', () => {
    const lines: string[] = []
    const logger = createLogger(
        { service: 'tachyon' },
        {
            level: 'trace',
            format: 'json',
            colorize: false,
            now: fixedNow,
            write: (line) => lines.push(line),
        },
    ).child({ scope: 'http', route: '/api/:id' })

    const err = new Error('boom')
    err.stack = 'Error: boom\n    at test'

    logger.error('Request failed', {
        method: 'GET',
        path: '/api/42',
        status: 500,
        err,
    })

    expect(lines).toHaveLength(1)

    const entry = JSON.parse(lines[0]) as Record<string, unknown>
    expect(entry.timestamp).toBe('2026-04-10T12:00:00.000Z')
    expect(entry.level).toBe('error')
    expect(entry.service).toBe('tachyon')
    expect(entry.scope).toBe('http')
    expect(entry.route).toBe('/api/:id')
    expect(entry.method).toBe('GET')
    expect(entry.path).toBe('/api/42')
    expect(entry.status).toBe(500)
    expect(entry.message).toBe('Request failed')
    expect(entry.err).toEqual({
        name: 'Error',
        message: 'boom',
        stack: 'Error: boom\n    at test',
    })
})
