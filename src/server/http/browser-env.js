// @ts-check

export const PUBLIC_BROWSER_ENV_PATH = '/browser-env.js';

/**
 * @param {string | undefined} value
 * @returns {string[]}
 */
function splitList(value) {
    return value
        ?.split(',')
        .map((entry) => entry.trim())
        .filter(Boolean)
        ?? [];
}

/**
 * Browser-visible env vars must be explicitly allowlisted. If browser code can
 * read a value, users can read it too, so this intentionally only exposes
 * public config.
 *
 * @returns {Record<string, string>}
 */
export function getPublicBrowserEnv() {
    /** @type {Record<string, string>} */
    const values = {};
    for (const key of splitList(process.env.TAC_PUBLIC_ENV)) {
        const value = process.env[key];
        if (typeof value === 'string') {
            values[key] = value;
        }
    }
    return values;
}

/** @returns {string} */
export function createPublicBrowserEnvScript() {
    const payload = JSON.stringify(getPublicBrowserEnv()).replace(/</g, '\\u003c');
    return `window.__ty_public_env__ = Object.freeze(${payload});\n`;
}

/** @returns {Response} */
export function createPublicBrowserEnvResponse() {
    return new Response(createPublicBrowserEnvScript(), {
        headers: {
            'Content-Type': 'application/javascript; charset=utf-8',
            'Cache-Control': 'no-cache, must-revalidate',
        },
    });
}

/**
 * @param {string} shellHTML
 * @returns {string}
 */
export function withPublicBrowserEnv(shellHTML) {
    const publicEnv = getPublicBrowserEnv();
    const keys = Object.keys(publicEnv);
    if (keys.length === 0 || shellHTML.includes(PUBLIC_BROWSER_ENV_PATH)) {
        return shellHTML;
    }
    const script = `    <script type="module" src="${PUBLIC_BROWSER_ENV_PATH}"></script>\n`;
    return shellHTML.replace('</head>', `${script}</head>`);
}
