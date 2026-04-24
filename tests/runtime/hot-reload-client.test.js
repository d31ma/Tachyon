// @ts-check
import { expect, test } from 'bun:test';
import { hmrFrameRequestsReload } from '../../src/runtime/hot-reload-client.js';
test('HMR client reloads only for explicit reload frames', () => {
    expect(hmrFrameRequestsReload('event: reload\n\n')).toBe(true);
    expect(hmrFrameRequestsReload('data: reload\n\n')).toBe(true);
    expect(hmrFrameRequestsReload('event: message\ndata: reload\n\n')).toBe(true);
    expect(hmrFrameRequestsReload(': connected\n\n')).toBe(false);
    expect(hmrFrameRequestsReload('Forbidden')).toBe(false);
    expect(hmrFrameRequestsReload('data: ping\n\n')).toBe(false);
    expect(hmrFrameRequestsReload('\n\n')).toBe(false);
});
