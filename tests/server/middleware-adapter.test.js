// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { mkdir, mkdtemp, rm } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import MiddlewareAdapter from '../../src/server/process/middleware-adapter.js';

/** @type {string[]} */
const tempDirs = [];

afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

/** @returns {Promise<string>} */
async function tempRoot() {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-middleware-adapter-'));
    tempDirs.push(root);
    return root;
}

const context = {
    requestId: '01ABCDEFGH',
    ipAddress: '127.0.0.1',
    protocol: 'http',
    host: 'localhost',
};

test('discovers class-style JavaScript middleware and runs before, after, and rateLimit phases', async () => {
    const root = await tempRoot();
    const middlewareDir = path.join(root, 'server', 'middleware');
    await mkdir(middlewareDir, { recursive: true });
    await Bun.write(path.join(middlewareDir, 'yon.js'), `
export class Middleware {
  static before(input) {
    if (input.request.headers['x-blocked'] === 'true') {
      return { action: 'respond', status: 403, headers: { 'content-type': 'application/json' }, body: { detail: 'blocked' } }
    }
    return { action: 'continue' }
  }

  static after(_input) {
    return { headers: { 'x-middleware': 'class-style' } }
  }

  static rateLimit(_input) {
    return { allowed: false, limit: 2, remaining: 0, resetAt: 12345, headers: { 'x-ratelimit-source': 'middleware' } }
  }
}
`);
    const adapter = await MiddlewareAdapter.discover(path.join(root, 'middleware'), root);
    expect(adapter).toBeTruthy();
    const hooks = adapter?.toRuntimeHooks();
    if (!hooks?.middleware || !hooks.rateLimiter)
        throw new Error('middleware hooks were not created');

    const early = await hooks.middleware.before?.(new Request('http://localhost/items', {
        headers: { 'x-blocked': 'true' },
    }), context);
    expect(early?.status).toBe(403);
    expect(await early?.json()).toEqual({ detail: 'blocked' });

    const after = await hooks.middleware.after?.(
        new Request('http://localhost/items'),
        Response.json({ ok: true }),
        context,
    );
    expect(after?.headers.get('x-middleware')).toBe('class-style');
    expect(await after?.json()).toEqual({ ok: true });

    const decision = await hooks.rateLimiter.take(new Request('http://localhost/items'), context);
    expect(decision).toEqual({
        allowed: false,
        limit: 2,
        remaining: 0,
        resetAt: 12345,
        headers: { 'x-ratelimit-source': 'middleware' },
    });
});

test('discovers raw protocol middleware for languages without class adapters', async () => {
    const root = await tempRoot();
    const middlewareDir = path.join(root, 'middleware');
    await mkdir(middlewareDir, { recursive: true });
    await Bun.write(path.join(middlewareDir, 'yon.js'), `
const input = await new Response(Bun.stdin.stream()).json()
Bun.stdout.write(JSON.stringify({
  action: 'respond',
  status: input.phase === 'before' ? 451 : 500,
  body: { phase: input.phase, requestId: input.context.requestId }
}))
`);
    const adapter = await MiddlewareAdapter.discover(path.join(root, 'middleware'), root);
    expect(adapter).toBeTruthy();
    const hooks = adapter?.toRuntimeHooks();
    if (!hooks?.middleware)
        throw new Error('middleware hooks were not created');

    const early = await hooks.middleware.before?.(new Request('http://localhost/raw'), context);
    expect(early?.status).toBe(451);
    expect(await early?.json()).toEqual({ phase: 'before', requestId: '01ABCDEFGH' });
});

test('class-style middleware only registers phases it implements', async () => {
    const root = await tempRoot();
    const middlewareDir = path.join(root, 'server', 'middleware');
    await mkdir(middlewareDir, { recursive: true });
    await Bun.write(path.join(middlewareDir, 'yon.js'), `
export class Middleware {
  static before(_input) {
    return { action: 'continue' }
  }
}
`);
    const adapter = await MiddlewareAdapter.discover(path.join(root, 'server', 'middleware'), root);
    const hooks = adapter?.toRuntimeHooks();
    if (!hooks?.middleware)
        throw new Error('middleware hooks were not created');

    expect(typeof hooks.middleware.before).toBe('function');
    expect(hooks.middleware.after).toBeUndefined();
    expect(hooks.rateLimiter).toBeNull();
});
