// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { installFakeOpfs } from './fake-opfs.js';
import * as fs from '../../src/runtime/fylo-opfs-fs.js';
import { __resetOpfsFs } from '../../src/runtime/fylo-opfs-fs.js';

const previousNavigator = globalThis.navigator;

afterEach(() => {
    __resetOpfsFs();
    Object.defineProperty(globalThis, 'navigator', { configurable: true, writable: true, value: previousNavigator });
});

test('mkdir(recursive) + writeFile + readFile round-trip (Buffer and text)', async () => {
    installFakeOpfs();
    await fs.mkdir('/db/.collections/users', { recursive: true });
    await fs.writeFile('/db/.collections/users/u1.json', '{"name":"Ada"}');

    const buffer = await fs.readFile('/db/.collections/users/u1.json');
    expect(Buffer.isBuffer(buffer)).toBe(true);
    expect(buffer.toString()).toBe('{"name":"Ada"}');
    expect(await fs.readFile('/db/.collections/users/u1.json', 'utf8')).toBe('{"name":"Ada"}');
});

test('open() FileHandle: random-access write/read, truncate, stat, sync', async () => {
    installFakeOpfs();
    const handle = await fs.open('/db/page.bin', 'w+');
    await handle.write(new Uint8Array([1, 2, 3, 4]), 0, 4, 0);
    await handle.write(new Uint8Array([9, 9]), 0, 2, 2); // overwrite at position 2

    const buffer = new Uint8Array(4);
    const { bytesRead } = await handle.read(buffer, 0, 4, 0);
    expect(bytesRead).toBe(4);
    expect([...buffer]).toEqual([1, 2, 9, 9]);
    expect((await handle.stat()).size).toBe(4);

    await handle.truncate(2);
    expect((await handle.stat()).size).toBe(2);
    await handle.sync();
    await handle.close();
});

test('readdir, stat kinds, rename, link, rm (incl. force + ENOENT)', async () => {
    installFakeOpfs();
    await fs.mkdir('/d', { recursive: true });
    await fs.writeFile('/d/a.txt', 'A');
    await fs.writeFile('/d/b.txt', 'B');

    expect((await fs.readdir('/d')).sort()).toEqual(['a.txt', 'b.txt']);
    expect((await fs.stat('/d')).isDirectory()).toBe(true);
    expect((await fs.stat('/d/a.txt')).isFile()).toBe(true);

    await fs.rename('/d/a.txt', '/d/c.txt');
    expect((await fs.readdir('/d')).sort()).toEqual(['b.txt', 'c.txt']);

    await fs.link('/d/b.txt', '/d/b2.txt');
    expect((await fs.readFile('/d/b2.txt')).toString()).toBe('B');

    await fs.rm('/d/c.txt');
    expect((await fs.readdir('/d')).sort()).toEqual(['b.txt', 'b2.txt']);

    await fs.rm('/does/not/exist', { force: true }); // must not throw
    await expect(fs.stat('/nope')).rejects.toThrow();
    await expect(fs.readFile('/d/missing.txt')).rejects.toThrow();
});

test('rename of a directory copies its contents and removes the source', async () => {
    installFakeOpfs();
    await fs.mkdir('/src/inner', { recursive: true });
    await fs.writeFile('/src/inner/x.txt', 'X');
    await fs.rename('/src', '/dest');

    expect((await fs.readFile('/dest/inner/x.txt')).toString()).toBe('X');
    await expect(fs.stat('/src')).rejects.toThrow();
});
