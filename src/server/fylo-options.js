// @ts-check

/**
 * Options for the vendored fylo Node shim (`src/vendor/fylo/fylo-node.mjs`).
 *
 * Fylo 26.28 is binary-first: the shim spawns `fylo exec --loop --root <root>`
 * and the compiled binary reads its own index/cache/backend configuration from
 * the environment (`FYLO_INDEX_BACKEND`, `FYLO_CACHE_*`, `FYLO_RLS`, …). So the
 * only per-instance options Tachyon passes are the binary location and WORM.
 *
 * @typedef {object} TachyonFyloOptions
 * @property {string=} binary  Path to the `fylo` binary (defaults to `fylo` on PATH).
 * @property {boolean=} worm   Reject mutations of existing documents (append-only).
 */

/**
 * @param {NodeJS.ProcessEnv} env
 * @param {string[]} names
 */
function envValue(env, names) {
    for (const name of names) {
        const value = env[name];
        if (typeof value === 'string' && value.trim()) return value.trim();
    }

    return undefined;
}

/**
 * Builds fylo Node-shim options from Tachyon's runtime environment.
 *
 * @param {string} _root
 * @param {NodeJS.ProcessEnv} [env]
 * @returns {TachyonFyloOptions}
 */
export function fyloOptions(_root, env = process.env) {
    /** @type {TachyonFyloOptions} */
    const options = {};

    const binary = envValue(env, ['FYLO_BINARY']);
    if (binary) options.binary = binary;

    if (envValue(env, ['FYLO_WORM']) === 'strict') options.worm = true;

    return options;
}
