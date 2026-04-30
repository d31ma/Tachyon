// @ts-check
import { expect, test } from 'bun:test';
import { fyloOptions } from '../../src/server/fylo-options.js';

test('fyloOptions keeps filesystem indexes by default', () => {
    expect(fyloOptions('/tmp/fylo', {})).toEqual({ root: '/tmp/fylo' });
});

test('fyloOptions enables FYLO s3-prefix indexes without bucket prefixes', () => {
    const options = fyloOptions('/tmp/fylo', {
        FYLO_INDEX_BACKEND: 's3-prefix',
        FYLO_S3_ACCESS_KEY_ID: 'access-key',
        FYLO_S3_SECRET_ACCESS_KEY: 'secret-key',
        FYLO_S3_SESSION_TOKEN: 'session-token',
        FYLO_S3_REGION: 'us-east-1',
        FYLO_S3_ENDPOINT: 'https://s3.example.test',
        FYLO_S3_BUCKET_PREFIX: 'legacy-prefix-',
    });

    expect(options).toEqual({
        root: '/tmp/fylo',
        index: {
            backend: 's3-prefix',
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
