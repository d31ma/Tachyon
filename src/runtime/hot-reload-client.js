// @ts-check
const HMR_RECONNECT_MS = 3000;
/** @param {string} frame */
export function hmrFrameRequestsReload(frame) {
    const normalized = frame.replaceAll('\r\n', '\n').trim();
    if (!normalized || normalized.startsWith(':'))
        return false;
    return /^(event:\s*reload|data:\s*reload)\s*$/im.test(normalized);
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
                if (hmrFrameRequestsReload(frame)) {
                    window.location.reload();
                    return;
                }
                frameEnd = buffer.indexOf('\n\n');
            }
        }
        buffer += decoder.decode();
        if (hmrFrameRequestsReload(buffer)) {
            window.location.reload();
            return;
        }
        setTimeout(connectHMR, HMR_RECONNECT_MS);
    }).catch(() => {
        setTimeout(connectHMR, HMR_RECONNECT_MS);
    });
}
if (typeof window !== 'undefined')
    connectHMR();
