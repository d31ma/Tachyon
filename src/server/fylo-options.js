// @ts-check

/**
 * @typedef {object} FyloS3IndexOptions
 * @property {string=} accessKeyId
 * @property {string=} secretAccessKey
 * @property {string=} sessionToken
 * @property {string=} endpoint
 * @property {string=} region
 *
 * @typedef {{ backend: 's3-prefix', s3?: FyloS3IndexOptions }} FyloS3IndexConfig
 * @typedef {{ root: string, index?: FyloS3IndexConfig }} TachyonFyloOptions
 */

/**
 * @param {NodeJS.ProcessEnv} env
 * @param {string[]} names
 * @returns {string | undefined}
 */
function envValue(env, names) {
    for (const name of names) {
        const value = env[name]?.trim();
        if (value) return value;
    }
    return undefined;
}

/**
 * @param {NodeJS.ProcessEnv} env
 * @returns {FyloS3IndexOptions}
 */
function s3Options(env) {
    return {
        accessKeyId: envValue(env, ['FYLO_S3_ACCESS_KEY_ID', 'AWS_ACCESS_KEY_ID']),
        secretAccessKey: envValue(env, ['FYLO_S3_SECRET_ACCESS_KEY', 'AWS_SECRET_ACCESS_KEY']),
        sessionToken: envValue(env, ['FYLO_S3_SESSION_TOKEN', 'AWS_SESSION_TOKEN']),
        endpoint: envValue(env, ['FYLO_S3_ENDPOINT', 'AWS_ENDPOINT_URL_S3', 'AWS_ENDPOINT_URL']),
        region: envValue(env, ['FYLO_S3_REGION', 'AWS_REGION', 'AWS_DEFAULT_REGION']),
    };
}

/**
 * Builds FYLO constructor options from Tachyon's runtime environment.
 *
 * FYLO 26.18.29 removed bucket-prefix configuration for S3 indexes. With
 * `s3-prefix`, each collection name is used directly as its S3 bucket name.
 *
 * @param {string} root
 * @param {NodeJS.ProcessEnv} [env]
 * @returns {TachyonFyloOptions}
 */
export function fyloOptions(root, env = process.env) {
    const backend = envValue(env, ['FYLO_INDEX_BACKEND']);
    if (backend !== 's3-prefix') {
        return { root };
    }

    return {
        root,
        index: {
            backend: 's3-prefix',
            s3: s3Options(env),
        },
    };
}
