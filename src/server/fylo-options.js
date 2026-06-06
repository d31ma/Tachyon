// @ts-check

/**
 * @typedef {object} FyloS3IndexOptions
 * @property {string=} accessKeyId
 * @property {string=} secretAccessKey
 * @property {string=} sessionToken
 * @property {string=} endpoint
 * @property {string=} region
 *
 * @typedef {{ backend: 'local-fs' }} FyloLocalFsIndexOptions
 * @typedef {{ backend: 's3-client', s3?: FyloS3IndexOptions }} FyloS3ClientIndexOptions
 * @typedef {FyloLocalFsIndexOptions | FyloS3ClientIndexOptions} FyloIndexOptions
 *
 * @typedef {object} FyloCacheOptions
 * @property {'memory' | 'redis'} backend
 * @property {'cache-aside' | 'read-through' | 'write-through' | 'write-around'} [method]
 * @property {number} [ttl]
 * @property {{ url?: string }} [redis]
 *
 * @typedef {'off' | 'strict'} FyloWormMode
 * @typedef {'await-sync' | 'fire-and-forget'} FyloSyncMode
 *
 * @typedef {object} TachyonFyloOptions
 * @property {FyloIndexOptions} index
 * @property {FyloCacheOptions=} cache
 * @property {boolean=} queue
 * @property {FyloWormMode=} worm
 * @property {FyloSyncMode=} syncMode
 * @property {boolean=} rls
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
 * @param {NodeJS.ProcessEnv} env
 * @returns {FyloS3IndexOptions | undefined}
 */
function fyloS3Options(env) {
    const options = {
        accessKeyId: envValue(env, ['AWS_ACCESS_KEY_ID', 'FYLO_S3_ACCESS_KEY_ID']),
        secretAccessKey: envValue(env, ['AWS_SECRET_ACCESS_KEY', 'FYLO_S3_SECRET_ACCESS_KEY']),
        sessionToken: envValue(env, ['AWS_SESSION_TOKEN', 'FYLO_S3_SESSION_TOKEN']),
        endpoint: envValue(env, ['AWS_ENDPOINT_URL_S3', 'AWS_ENDPOINT_URL', 'FYLO_S3_ENDPOINT']),
        region: envValue(env, ['AWS_REGION', 'AWS_DEFAULT_REGION', 'FYLO_S3_REGION']),
    };
    const entries = Object.entries(options).filter(([, value]) => value !== undefined);

    return entries.length ? Object.fromEntries(entries) : undefined;
}

/**
 * @param {string | undefined} value
 * @returns {boolean}
 */
function isTruthy(value) {
    const normalized = value?.trim().toLowerCase();
    return normalized === '1' || normalized === 'true' || normalized === 'yes' || normalized === 'on';
}

/**
 * @param {NodeJS.ProcessEnv} env
 * @returns {FyloCacheOptions | undefined}
 */
function fyloCacheOptions(env) {
    const backend = envValue(env, ['FYLO_CACHE_BACKEND']);
    if (!backend) return undefined;

    if (backend === 'memory') {
        return {
            backend: 'memory',
            method: /** @type {FyloCacheOptions['method']} */ (envValue(env, ['FYLO_CACHE_METHOD']) ?? 'cache-aside'),
            ttl: Number(envValue(env, ['FYLO_CACHE_TTL']) ?? 30),
        };
    }

    if (backend === 'redis') {
        return {
            backend: 'redis',
            method: /** @type {FyloCacheOptions['method']} */ (envValue(env, ['FYLO_CACHE_METHOD']) ?? 'cache-aside'),
            ttl: Number(envValue(env, ['FYLO_CACHE_TTL']) ?? 60),
            redis: { url: envValue(env, ['FYLO_REDIS_URL']) },
        };
    }

    return undefined;
}

/**
 * Builds FYLO constructor options from Tachyon's runtime environment.
 *
 * FYLO 26.23 accepts the database root as the first constructor argument and
 * index/backend configuration as the second. Tachyon makes the local-fs default
 * explicit so deployments can audit the selected index backend from env alone.
 *
 * @param {string} _root
 * @param {NodeJS.ProcessEnv} [env]
 * @returns {TachyonFyloOptions}
 */
export function fyloOptions(_root, env = process.env) {
    const backend = envValue(env, ['FYLO_INDEX_BACKEND']) ?? 'local-fs';

    if (backend !== 'local-fs' && backend !== 's3-client') {
        throw new Error(`Unsupported FYLO_INDEX_BACKEND "${backend}". Use "local-fs" or "s3-client".`);
    }

    /** @type {TachyonFyloOptions} */
    const options = backend === 's3-client'
        ? { index: { backend: 's3-client', ...(fyloS3Options(env) ? { s3: fyloS3Options(env) } : {}) } }
        : { index: { backend: 'local-fs' } };

    const cache = fyloCacheOptions(env);
    if (cache) options.cache = cache;

    if (isTruthy(envValue(env, ['FYLO_QUEUE']))) options.queue = true;

    const wormMode = /** @type {FyloWormMode | undefined} */ (envValue(env, ['FYLO_WORM']));
    if (wormMode === 'strict') options.worm = wormMode;

    const syncMode = /** @type {FyloSyncMode | undefined} */ (envValue(env, ['FYLO_SYNC_MODE']));
    if (syncMode === 'await-sync' || syncMode === 'fire-and-forget') options.syncMode = syncMode;

    if (isTruthy(envValue(env, ['FYLO_RLS']))) options.rls = true;

    return options;
}
