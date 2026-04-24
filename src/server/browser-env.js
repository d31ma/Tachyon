// @ts-check

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

/**
 * @param {string} shellHTML
 * @returns {string}
 */
export function withPublicBrowserEnv(shellHTML) {
    const publicEnv = getPublicBrowserEnv();
    const keys = Object.keys(publicEnv);
    if (keys.length === 0 || shellHTML.includes('window.__ty_public_env__')) {
        return shellHTML;
    }
    const payload = JSON.stringify(publicEnv).replace(/</g, '\\u003c');
    const script = `    <script>window.__ty_public_env__ = Object.freeze(${payload});</script>\n`;
    return shellHTML.replace('</head>', `${script}</head>`);
}
