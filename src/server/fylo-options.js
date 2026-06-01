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
 * @typedef {{ index: FyloIndexOptions }} TachyonFyloOptions
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
 * Builds FYLO constructor options from Tachyon's runtime environment.
 *
 * FYLO 26.22 accepts the database root as the first constructor argument and
 * index/backend configuration as the second. Tachyon makes the local-fs default
 * explicit so deployments can audit the selected index backend from env alone.
 *
 * @param {string} _root
 * @param {NodeJS.ProcessEnv} [env]
 * @returns {TachyonFyloOptions}
 */
export function fyloOptions(_root, env = process.env) {
    const backend = envValue(env, ['FYLO_INDEX_BACKEND']) ?? 'local-fs';

    if (backend === 'local-fs') {
        return { index: { backend: 'local-fs' } };
    }

    if (backend === 's3-client') {
        const s3 = fyloS3Options(env);
        return s3 ? { index: { backend: 's3-client', s3 } } : { index: { backend: 's3-client' } };
    }

    throw new Error(`Unsupported FYLO_INDEX_BACKEND "${backend}". Use "local-fs" or "s3-client".`);
}
