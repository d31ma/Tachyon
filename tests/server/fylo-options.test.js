// @ts-check
import { expect, test } from 'bun:test';
import { fyloOptions } from '../../src/server/fylo-options.js';

// Fylo 26.28 is binary-first: the shim spawns `fylo exec --loop` and the binary
// reads index/cache/backend config from its own env. Tachyon only passes the
// binary location and WORM through the Node-shim constructor.

test('fyloOptions defaults to empty (fylo on PATH, WORM off)', () => {
    expect(fyloOptions('/tmp/fylo', {})).toEqual({});
});

test('fyloOptions passes through an explicit binary path', () => {
    expect(fyloOptions('/tmp/fylo', { FYLO_BINARY: '/opt/bin/fylo' })).toEqual({
        binary: '/opt/bin/fylo',
    });
});

test('fyloOptions enables WORM when FYLO_WORM=strict', () => {
    expect(fyloOptions('/tmp/fylo', { FYLO_WORM: 'strict' })).toEqual({ worm: true });
});

test('fyloOptions ignores non-strict FYLO_WORM values', () => {
    expect(fyloOptions('/tmp/fylo', { FYLO_WORM: 'off' })).toEqual({});
});

test('fyloOptions combines binary and WORM', () => {
    expect(fyloOptions('/tmp/fylo', { FYLO_BINARY: 'fylo', FYLO_WORM: 'strict' })).toEqual({
        binary: 'fylo',
        worm: true,
    });
});
