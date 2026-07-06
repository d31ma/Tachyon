// @ts-check
const HMR_RECONNECT_MS = 3000;

/**
 * Parse an SSE frame for an HMR event.
 * @param {string} frame
 * @returns {{ reload: boolean, update: boolean, paths: string[] }}
 */
function parseHmrFrame(frame) {
    const normalized = frame.replaceAll('\r\n', '\n').trim();
    if (!normalized || normalized.startsWith(':')) return { reload: false, update: false, paths: [] };

    const lines = normalized.split('\n');
    let event = '';
    let data = '';
    for (const line of lines) {
        if (line.startsWith('event:')) event = line.slice(6).trim();
        if (line.startsWith('data:')) data = line.slice(5).trim();
    }

    if (event === 'reload' || data === 'reload') {
        return { reload: true, update: false, paths: [] };
    }
    if (event === 'update' && data) {
        try {
            const payload = JSON.parse(data);
            const paths = Array.isArray(payload.paths) ? payload.paths : [];
            return { reload: false, update: paths.length > 0, paths };
        } catch { /* malformed */ }
    }
    return { reload: false, update: false, paths: [] };
}

/**
 * Targeted HMR update — re-import only the changed modules. Falls back
 * to a full soft-reload if the runtime doesn't support targeted updates.
 * @param {string[]} paths
 */
async function targetedUpdate(paths) {
    if (typeof window.__tachyon_hmr_update__ === 'function') {
        await window.__tachyon_hmr_update__(paths);
        return;
    }
    // Fallback to full soft-reload
    if (typeof window.__tachyon_hmr_reload__ === 'function') {
        await window.__tachyon_hmr_reload__();
        return;
    }
    window.location.reload();
}

/**
 * Full soft-reload — invalidate all caches and re-initialize.
 */
async function softReload() {
    if (typeof window.__tachyon_hmr_reload__ === 'function') {
        await window.__tachyon_hmr_reload__();
        return;
    }
    window.location.reload();
}

function connectHMR() {
    fetch('/hmr').then(async (res) => {
        if (!res.ok || !res.body) {
            setTimeout(connectHMR, HMR_RECONNECT_MS);
            return;
        }
        const decoder = new TextDecoder();
        let buffer = '';
        for await (const chunk of res.body) {
            buffer += decoder.decode(chunk, { stream: true });
            let frameEnd = buffer.indexOf('\n\n');
            while (frameEnd !== -1) {
                const frame = buffer.slice(0, frameEnd);
                buffer = buffer.slice(frameEnd + 2);
                const { reload, update, paths } = parseHmrFrame(frame);
                if (update && paths.length > 0) {
                    targetedUpdate(paths);
                    return;
                }
                if (reload) {
                    softReload();
                    return;
                }
                frameEnd = buffer.indexOf('\n\n');
            }
        }
        buffer += decoder.decode();
        const { reload, update, paths } = parseHmrFrame(buffer);
        if (update && paths.length > 0) {
            targetedUpdate(paths);
            return;
        }
        if (reload) {
            softReload();
            return;
        }
        setTimeout(connectHMR, HMR_RECONNECT_MS);
    }).catch(() => {
        setTimeout(connectHMR, HMR_RECONNECT_MS);
    });
}
if (typeof window !== 'undefined')
    connectHMR();

// Legacy export for tests — wraps the new parseHmrFrame to return
// the same boolean "does this frame request a reload" signal.
/** @param {string} frame */
export function hmrFrameRequestsReload(frame) {
    const { reload, update } = parseHmrFrame(frame);
    return reload || update;
}
