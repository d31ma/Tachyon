// @ts-check
import { expect, test } from 'bun:test';
import { createLogger } from '../../src/server/observability/logger.js';
const fixedNow = () => new Date('2026-04-10T12:00:00.000Z');
test('pretty logger shows time, level, brand tag, message, and structured fields', () => {
    /** @type {string[]} */
    const lines = [];
    const logger = createLogger({ service: 'tachyon', scope: 'cli:bundle' }, {
        level: 'debug',
        format: 'pretty',
        colorize: false,
        now: fixedNow,
        /** @param {string} line */
        write: (line) => lines.push(line),
    });
    logger.info('Bundle completed', {
        routeCount: 7,
        distPath: '/tmp/dist',
        durationMs: 18,
    });
    expect(lines).toHaveLength(1);
    // Time only, no date noise.
    expect(lines[0]).toContain('12:00:00.000');
    expect(lines[0]).not.toContain('2026-04-10');
    expect(lines[0]).toContain('INFO ');
    // cli:bundle is grouped under the Tac product surface.
    expect(lines[0]).toContain('[Tac]');
    expect(lines[0]).not.toContain('tachyon/');
    expect(lines[0]).toContain('Bundle completed');
    expect(lines[0]).toContain('routeCount=7');
    expect(lines[0]).toContain('distPath=/tmp/dist');
    expect(lines[0]).toContain('durationMs=18');
    expect(lines[0]).not.toContain('pid=');
});
test('pretty logger falls back to the raw scope when it is not a known brand', () => {
    /** @type {string[]} */
    const lines = [];
    const logger = createLogger({ service: 'tachyon', scope: 'custom-thing' }, {
        level: 'info', format: 'pretty', colorize: false, now: fixedNow,
        write: (line) => lines.push(line),
    });
    logger.info('Hello');
    expect(lines[0]).toContain('[custom-thing]');
});
test('logger respects configured level filtering', () => {
    /** @type {string[]} */
    const lines = [];
    const logger = createLogger({ service: 'tachyon' }, {
        level: 'warn',
        format: 'pretty',
        colorize: false,
        now: fixedNow,
        /** @param {string} line */
        write: (line) => lines.push(line),
    });
    logger.info('Suppressed');
    logger.warn('Visible warning');
    expect(lines).toHaveLength(1);
    expect(lines[0]).toContain('Visible warning');
    expect(lines[0]).not.toContain('Suppressed');
});
test('json logger serializes child bindings and errors', () => {
    /** @type {string[]} */
    const lines = [];
    const logger = createLogger({ service: 'tachyon' }, {
        level: 'trace',
        format: 'json',
        colorize: false,
        now: fixedNow,
        /** @param {string} line */
        write: (line) => lines.push(line),
    }).child({ scope: 'http', route: '/api/:id' });
    const err = new Error('boom');
    err.stack = 'Error: boom\n    at test';
    logger.error('Request failed', {
        method: 'GET',
        path: '/api/42',
        status: 500,
        err,
    });
    expect(lines).toHaveLength(1);
    const entry = JSON.parse(lines[0]);
    expect(entry.timestamp).toBe('2026-04-10T12:00:00.000Z');
    expect(entry.level).toBe('error');
    expect(entry.service).toBe('tachyon');
    expect(entry.scope).toBe('http');
    expect(entry.route).toBe('/api/:id');
    expect(entry.method).toBe('GET');
    expect(entry.path).toBe('/api/42');
    expect(entry.status).toBe(500);
    expect(entry.message).toBe('Request failed');
    expect(entry.err).toEqual({
        name: 'Error',
        message: 'boom',
        stack: 'Error: boom\n    at test',
    });
});
