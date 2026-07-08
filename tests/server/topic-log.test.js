import { test, expect } from 'bun:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import TopicLog from '../../src/server/realtime/topic-log.js';

test('publish then readTopic preserves order and resumes from position', async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'topic-log-'));
    try {
        const log = new TopicLog(dir);
        await log.publish('t', { event: 'a' });
        await log.publish('t', { event: 'b' });

        const first = await log.readTopic('t', 0, 100);
        expect(first.map((r) => r.message.payload.event)).toEqual(['a', 'b']);
        expect(first[1].nextPosition).toBe(2);

        // resume from the cursor: no already-read records
        expect(await log.readTopic('t', first[1].nextPosition, 100)).toEqual([]);

        await log.publish('t', { event: 'c' });
        const next = await log.readTopic('t', first[1].nextPosition, 100);
        expect(next.map((r) => r.message.payload.event)).toEqual(['c']);

        // limit is honored
        expect((await log.readTopic('t', 0, 1)).length).toBe(1);
        // unknown topic reads empty
        expect(await log.readTopic('missing', 0, 100)).toEqual([]);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});
