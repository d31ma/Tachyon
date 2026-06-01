// @ts-check
import { expect, test } from 'bun:test';
import { fyloOptions } from '../../src/server/fylo-options.js';

test('fyloOptions defaults to FYLO local-fs indexing', () => {
    expect(fyloOptions('/tmp/fylo', {})).toEqual({
        index: { backend: 'local-fs' },
    });
});

test('fyloOptions accepts explicit FYLO local-fs indexing', () => {
    expect(fyloOptions('/tmp/fylo', { FYLO_INDEX_BACKEND: 'local-fs' })).toEqual({
        index: { backend: 'local-fs' },
    });
});

test('fyloOptions passes through FYLO s3-client indexing', () => {
    const options = fyloOptions('/tmp/fylo', {
        FYLO_INDEX_BACKEND: 's3-client',
        FYLO_S3_ACCESS_KEY_ID: 'access-key',
        FYLO_S3_SECRET_ACCESS_KEY: 'secret-key',
        FYLO_S3_SESSION_TOKEN: 'session-token',
        FYLO_S3_REGION: 'us-east-1',
        FYLO_S3_ENDPOINT: 'https://s3.example.test',
    });

    expect(options).toEqual({
        index: {
            backend: 's3-client',
            s3: {
                accessKeyId: 'access-key',
                secretAccessKey: 'secret-key',
                sessionToken: 'session-token',
                region: 'us-east-1',
                endpoint: 'https://s3.example.test',
            },
        },
    });
});

test('fyloOptions rejects removed FYLO s3-prefix indexing', () => {
    expect(() => fyloOptions('/tmp/fylo', { FYLO_INDEX_BACKEND: 's3-prefix' })).toThrow(
        'Unsupported FYLO_INDEX_BACKEND "s3-prefix"'
    );
});
