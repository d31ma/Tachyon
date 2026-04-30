#!/usr/bin/env bun
// @ts-check

import path from 'node:path';

const repoRoot = path.resolve(import.meta.dir, '..');
const defaultEnvFile = path.join(repoRoot, 'examples', '.env.localstack');
const defaultSchemaDir = path.join(repoRoot, 'examples', 'db', 'schemas');

/**
 * @param {string} filePath
 * @returns {Promise<Record<string, string>>}
 */
async function loadEnvFile(filePath) {
    const values = /** @type {Record<string, string>} */ ({});
    const file = Bun.file(filePath);
    if (!(await file.exists())) return values;

    const text = await file.text();
    for (const line of text.split(/\r?\n/)) {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith('#') || !trimmed.includes('=')) continue;
        const [name, ...rest] = trimmed.split('=');
        values[name.trim()] = rest.join('=').trim();
    }
    return values;
}

/**
 * @param {string[]} command
 * @param {Record<string, string | undefined>} env
 * @returns {Promise<{ exitCode: number, stdout: string, stderr: string }>}
 */
async function run(command, env) {
    const process = Bun.spawn(command, {
        cwd: repoRoot,
        env: { ...Bun.env, ...env },
        stdout: 'pipe',
        stderr: 'pipe',
    });
    const [stdout, stderr, exitCode] = await Promise.all([
        new Response(process.stdout).text(),
        new Response(process.stderr).text(),
        process.exited,
    ]);
    return { exitCode, stdout, stderr };
}

/**
 * @param {Record<string, string>} config
 * @returns {Record<string, string | undefined>}
 */
function awsEnv(config) {
    return {
        AWS_ACCESS_KEY_ID: config.FYLO_S3_ACCESS_KEY_ID || 'test',
        AWS_SECRET_ACCESS_KEY: config.FYLO_S3_SECRET_ACCESS_KEY || 'test',
        AWS_SESSION_TOKEN: config.FYLO_S3_SESSION_TOKEN || undefined,
        AWS_DEFAULT_REGION: config.FYLO_S3_REGION || 'us-east-1',
    };
}

/**
 * @param {Record<string, string>} config
 * @returns {string[]}
 */
function endpointArgs(config) {
    const endpoint = config.FYLO_S3_ENDPOINT || 'http://localhost:4566';
    return ['--endpoint-url', endpoint];
}

/**
 * @param {string} schemaDir
 * @returns {Promise<string[]>}
 */
async function collectionNames(schemaDir) {
    const entries = await Array.fromAsync(new Bun.Glob('*/').scan({ cwd: schemaDir, onlyFiles: false }));
    return entries.map((entry) => entry.replace(/\/$/, '')).filter(Boolean).sort();
}

/**
 * @param {string} bucket
 * @param {Record<string, string>} config
 * @param {Record<string, string | undefined>} env
 */
async function ensureBucket(bucket, config, env) {
    const common = [...endpointArgs(config), '--bucket', bucket];
    const head = await run(['aws', 's3api', 'head-bucket', ...common], env);
    if (head.exitCode === 0) {
        console.log(`bucket exists: ${bucket}`);
        return;
    }

    const create = await run(['aws', 's3api', 'create-bucket', ...common], env);
    if (create.exitCode !== 0) {
        throw new Error(`failed to create ${bucket}: ${create.stderr.trim() || create.stdout.trim()}`);
    }
    console.log(`bucket created: ${bucket}`);
}

const envFile = path.resolve(Bun.env.LOCALSTACK_ENV_FILE || defaultEnvFile);
const schemaDir = path.resolve(Bun.env.FYLO_SCHEMA_DIR || defaultSchemaDir);
const config = await loadEnvFile(envFile);
const env = awsEnv(config);
const names = await collectionNames(schemaDir);

if (!names.length) {
    throw new Error(`No FYLO collection schemas found in ${schemaDir}`);
}

const identity = await run(['aws', 'sts', 'get-caller-identity', ...endpointArgs(config), '--output', 'json'], env);
if (identity.exitCode !== 0) {
    throw new Error(`LocalStack is not ready or AWS CLI cannot connect: ${identity.stderr.trim()}`);
}

console.log(`LocalStack endpoint: ${config.FYLO_S3_ENDPOINT || 'http://localhost:4566'}`);
for (const name of names) {
    await ensureBucket(name, config, env);
}
