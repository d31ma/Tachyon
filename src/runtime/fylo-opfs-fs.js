// @ts-check
//
// OPFS-backed `node:fs/promises` used by the vendored FYLO browser engine.
//
// This is the Virtual File System that lets the *real* FYLO engine run inside a
// browser Web Worker unchanged: bundle FYLO with `node:fs/promises` aliased to
// this module, and its document/page writes land in the Origin Private File
// System. Random-access `open()` FileHandles are backed by OPFS synchronous
// access handles (`createSyncAccessHandle`) — fast, in-place, durable byte I/O,
// which is exactly FYLO's storage model and is only permitted inside a Worker.
//
// FYLO's fs surface (from its imports): mkdir, open, readFile, writeFile,
// readdir, rename, rm, unlink, link, stat, chmod, utimes.
//
// STATUS: groundwork, not yet wired into the runtime. This VFS is proven for
// FYLO's *writes* (real FYLO produces its exact on-disk format through it — see
// tests/runtime/fylo-opfs-fs.test.js), but running FYLO unmodified in a browser
// also needs a Bun-runtime shim: FYLO reads/hashes/IDs via Bun.file,
// Bun.CryptoHasher (synchronous SHA), Bun.mmap, Bun.randomUUIDv7, Bun.JSONL,
// Bun.sleep, Bun.spawn. The cleaner long-term fix is to abstract FYLO's
// runtime/storage backend in FYLO itself. Until then the browser store
// remains fylo-local.js (a JS reimplementation over OPFS sync access handles).

/** @type {Promise<FileSystemDirectoryHandle> | null} */
let cachedRoot = null;

/** @returns {Promise<FileSystemDirectoryHandle>} */
function opfsRoot() {
    if (!cachedRoot) {
        const storage = /** @type {{ storage?: { getDirectory?: () => Promise<FileSystemDirectoryHandle> } }} */ (
            /** @type {unknown} */ (globalThis.navigator)
        ).storage;
        if (!storage?.getDirectory)
            throw fsError('ENOSYS', 'OPFS is not available in this context (navigator.storage.getDirectory missing)');
        cachedRoot = storage.getDirectory();
    }
    return cachedRoot;
}

/** Reset the cached OPFS root (tests / fresh origins). */
export function __resetOpfsFs() {
    cachedRoot = null;
}

/**
 * @param {string} code
 * @param {string} message
 * @returns {Error & { code: string }}
 */
function fsError(code, message) {
    const error = /** @type {Error & { code: string }} */ (new Error(message));
    error.code = code;
    return error;
}

/** @param {import('fs').PathLike} path @returns {string[]} */
function splitPath(path) {
    const text = path instanceof URL ? path.pathname : String(path);
    /** @type {string[]} */
    const parts = [];
    for (const segment of text.split('/')) {
        if (!segment || segment === '.') continue;
        if (segment === '..') { parts.pop(); continue; }
        parts.push(segment);
    }
    return parts;
}

/**
 * @param {string[]} parts
 * @param {{ create: boolean }} options
 * @returns {Promise<FileSystemDirectoryHandle>}
 */
async function resolveDir(parts, options) {
    let dir = await opfsRoot();
    for (const part of parts) {
        try {
            dir = await dir.getDirectoryHandle(part, { create: options.create });
        } catch {
            throw fsError('ENOENT', `ENOENT: no such file or directory, '${parts.join('/')}'`);
        }
    }
    return dir;
}

/**
 * @param {import('fs').PathLike} path
 * @param {{ create: boolean }} options
 * @returns {Promise<{ dir: FileSystemDirectoryHandle, name: string }>}
 */
async function resolveParent(path, options) {
    const parts = splitPath(path);
    const name = parts.pop();
    if (name === undefined)
        throw fsError('EINVAL', `EINVAL: invalid path '${path}'`);
    const dir = await resolveDir(parts, options);
    return { dir, name };
}

/** @param {unknown} data @param {BufferEncoding} [encoding] @returns {Uint8Array} */
function toBytes(data, encoding) {
    if (typeof data === 'string')
        return new TextEncoder().encode(data);
    if (data instanceof Uint8Array)
        return data;
    if (data instanceof ArrayBuffer)
        return new Uint8Array(data);
    if (ArrayBuffer.isView(data))
        return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
    return new TextEncoder().encode(String(data));
}

/**
 * Minimal type for the OPFS synchronous access handle (not yet in lib.dom).
 * @typedef {{ getSize(): number, read(b: ArrayBufferView, o?: { at?: number }): number, write(b: ArrayBufferView, o?: { at?: number }): number, truncate(size: number): void, flush(): void, close(): void }} SyncAccessHandle
 */

/**
 * @param {FileSystemFileHandle} handle
 * @returns {Promise<SyncAccessHandle>}
 */
async function openSyncAccess(handle) {
    const candidate = /** @type {{ createSyncAccessHandle?: () => Promise<SyncAccessHandle> }} */ (handle);
    if (typeof candidate.createSyncAccessHandle !== 'function')
        throw fsError('ENOSYS', 'OPFS synchronous access handles are only available inside a Worker');
    return candidate.createSyncAccessHandle();
}

/**
 * Node FileHandle subset backed by an OPFS synchronous access handle.
 * Implements the random-access I/O FYLO performs on page files.
 */
class OpfsFileHandle {
    /** @param {SyncAccessHandle} access */
    constructor(access) {
        this.access = access;
        this.position = 0;
        this.closed = false;
    }

    /**
     * @param {ArrayBufferView | { buffer?: ArrayBufferView, offset?: number, length?: number, position?: number | null }} bufferOrOptions
     * @param {number} [offset]
     * @param {number} [length]
     * @param {number | null} [position]
     */
    async read(bufferOrOptions, offset, length, position) {
        let view = /** @type {ArrayBufferView | undefined} */ (bufferOrOptions);
        if (bufferOrOptions && !ArrayBuffer.isView(bufferOrOptions)) {
            const options = /** @type {{ buffer?: ArrayBufferView, offset?: number, length?: number, position?: number | null }} */ (bufferOrOptions);
            view = options.buffer;
            offset = options.offset;
            length = options.length;
            position = options.position ?? null;
        }
        if (!view)
            view = new Uint8Array(0);
        const start = offset ?? 0;
        const count = length ?? (view.byteLength - start);
        const at = position == null ? this.position : position;
        const target = new Uint8Array(view.buffer, view.byteOffset + start, count);
        const bytesRead = this.access.read(target, { at });
        if (position == null)
            this.position = at + bytesRead;
        return { bytesRead, buffer: view };
    }

    /**
     * @param {ArrayBufferView | string} data
     * @param {number | { offset?: number, length?: number, position?: number | null }} [offsetOrOptions]
     * @param {number | BufferEncoding} [lengthOrEncoding]
     * @param {number | null} [position]
     */
    async write(data, offsetOrOptions, lengthOrEncoding, position) {
        /** @type {Uint8Array} */
        let bytes;
        let at;
        if (typeof data === 'string') {
            bytes = toBytes(data, typeof lengthOrEncoding === 'string' ? lengthOrEncoding : undefined);
            at = typeof offsetOrOptions === 'number' ? offsetOrOptions : this.position;
        }
        else {
            let offset = 0;
            let length = data.byteLength;
            if (typeof offsetOrOptions === 'object' && offsetOrOptions) {
                offset = offsetOrOptions.offset ?? 0;
                length = offsetOrOptions.length ?? data.byteLength - offset;
                position = offsetOrOptions.position ?? null;
            }
            else {
                offset = typeof offsetOrOptions === 'number' ? offsetOrOptions : 0;
                length = typeof lengthOrEncoding === 'number' ? lengthOrEncoding : data.byteLength - offset;
            }
            bytes = new Uint8Array(data.buffer, data.byteOffset + offset, length);
            at = position == null ? this.position : position;
        }
        const bytesWritten = this.access.write(bytes, { at });
        this.position = at + bytesWritten;
        return { bytesWritten, buffer: data };
    }

    /** @param {number} [length] */
    async truncate(length = 0) {
        this.access.truncate(length);
    }

    async stat() {
        return makeStats('file', this.access.getSize());
    }

    async sync() {
        this.access.flush();
    }

    async datasync() {
        this.access.flush();
    }

    /** @param {{ encoding?: BufferEncoding } | BufferEncoding} [options] */
    async readFile(options) {
        const size = this.access.getSize();
        const buffer = new Uint8Array(size);
        if (size > 0)
            this.access.read(buffer, { at: 0 });
        const encoding = typeof options === 'string' ? options : options?.encoding;
        return encoding ? new TextDecoder().decode(buffer) : Buffer.from(buffer);
    }

    /** @param {ArrayBufferView | string} data */
    async writeFile(data) {
        const bytes = toBytes(data);
        this.access.truncate(0);
        this.access.write(bytes, { at: 0 });
        this.access.flush();
    }

    async close() {
        if (this.closed) return;
        this.closed = true;
        this.access.flush();
        this.access.close();
    }
}

/**
 * No-op handle returned when FYLO opens a *directory* (`open(dir, 'r')`) to
 * fsync it after a durable rename. OPFS has no directory fsync and doesn't need
 * one — file writes are flushed in place — so sync/close are no-ops.
 */
class OpfsDirectoryHandle {
    async sync() { /* directories are durable in OPFS without an explicit fsync */ }
    async datasync() { /* see sync() */ }
    async stat() { return makeStats('directory', 0); }
    async close() { /* nothing to release */ }
    async read() { throw fsError('EISDIR', 'EISDIR: illegal operation on a directory, read'); }
    async write() { throw fsError('EISDIR', 'EISDIR: illegal operation on a directory, write'); }
}

/**
 * @param {'file' | 'directory'} kind
 * @param {number} size
 * @param {number} [mtimeMs]
 */
function makeStats(kind, size, mtimeMs = Date.now()) {
    const date = new Date(mtimeMs);
    return {
        isFile: () => kind === 'file',
        isDirectory: () => kind === 'directory',
        isSymbolicLink: () => false,
        isBlockDevice: () => false,
        isCharacterDevice: () => false,
        isFIFO: () => false,
        isSocket: () => false,
        size,
        mode: kind === 'directory' ? 0o040755 : 0o100644,
        dev: 0, ino: 0, nlink: 1, uid: 0, gid: 0, rdev: 0,
        blksize: 4096, blocks: Math.ceil(size / 512),
        atimeMs: mtimeMs, mtimeMs, ctimeMs: mtimeMs, birthtimeMs: mtimeMs,
        atime: date, mtime: date, ctime: date, birthtime: date,
    };
}

// ── node:fs/promises surface ────────────────────────────────────────────────

/**
 * @param {import('fs').PathLike} path
 * @param {string} [flags]
 * @returns {Promise<OpfsFileHandle | OpfsDirectoryHandle>}
 */
export async function open(path, flags = 'r') {
    const text = String(flags);
    const create = /[wax+]/.test(text);
    const { dir, name } = await resolveParent(path, { create });
    /** @type {FileSystemFileHandle} */
    let handle;
    try {
        handle = await dir.getFileHandle(name, { create });
    } catch {
        // FYLO opens a directory (`open(dir, 'r')`) to fsync it after a durable
        // rename; return a no-op directory handle in that case.
        try {
            await dir.getDirectoryHandle(name);
            return new OpfsDirectoryHandle();
        } catch {
            throw fsError('ENOENT', `ENOENT: no such file or directory, open '${path}'`);
        }
    }
    const access = await openSyncAccess(handle);
    const fileHandle = new OpfsFileHandle(access);
    if (text.startsWith('w')) {
        access.truncate(0);
    }
    else if (text.startsWith('a')) {
        fileHandle.position = access.getSize();
    }
    return fileHandle;
}

/**
 * @param {import('fs').PathLike} path
 * @param {{ recursive?: boolean } | undefined} [options]
 */
export async function mkdir(path, options) {
    const parts = splitPath(path);
    if (options?.recursive) {
        let dir = await opfsRoot();
        for (const part of parts)
            dir = await dir.getDirectoryHandle(part, { create: true });
        return undefined;
    }
    const name = parts.pop();
    if (name === undefined)
        return undefined;
    const dir = await resolveDir(parts, { create: false });
    await dir.getDirectoryHandle(name, { create: true });
    return undefined;
}

/**
 * @param {import('fs').PathLike} path
 * @param {{ withFileTypes?: boolean } | BufferEncoding} [options]
 */
export async function readdir(path, options) {
    const dir = await resolveDir(splitPath(path), { create: false });
    const withFileTypes = typeof options === 'object' && options?.withFileTypes;
    /** @type {Array<string | { name: string, isFile(): boolean, isDirectory(): boolean }>} */
    const entries = [];
    const entriesDir = /** @type {{ entries(): AsyncIterable<[string, FileSystemHandle]> }} */ (/** @type {unknown} */ (dir));
    for await (const [name, handle] of entriesDir.entries()) {
        if (!withFileTypes) {
            entries.push(name);
            continue;
        }
        const isDir = handle.kind === 'directory';
        entries.push({ name, isFile: () => !isDir, isDirectory: () => isDir });
    }
    return entries;
}

/**
 * @param {import('fs').PathLike} path
 * @param {{ encoding?: BufferEncoding } | BufferEncoding} [options]
 */
export async function readFile(path, options) {
    const { dir, name } = await resolveParent(path, { create: false });
    /** @type {FileSystemFileHandle} */
    let handle;
    try {
        handle = await dir.getFileHandle(name);
    } catch {
        throw fsError('ENOENT', `ENOENT: no such file or directory, open '${path}'`);
    }
    const bytes = new Uint8Array(await (await handle.getFile()).arrayBuffer());
    const encoding = typeof options === 'string' ? options : options?.encoding;
    return encoding ? new TextDecoder().decode(bytes) : Buffer.from(bytes);
}

/**
 * @param {import('fs').PathLike} path
 * @param {string | ArrayBufferView} data
 * @param {{ encoding?: BufferEncoding } | BufferEncoding} [options]
 */
export async function writeFile(path, data, options) {
    const { dir, name } = await resolveParent(path, { create: true });
    const handle = await dir.getFileHandle(name, { create: true });
    const encoding = typeof options === 'string' ? options : options?.encoding;
    const bytes = toBytes(data, encoding);
    const access = await openSyncAccess(handle);
    try {
        access.truncate(0);
        access.write(bytes, { at: 0 });
        access.flush();
    } finally {
        access.close();
    }
}

/** @param {import('fs').PathLike} path */
export async function stat(path) {
    const parts = splitPath(path);
    if (parts.length === 0)
        return makeStats('directory', 0);
    const name = parts[parts.length - 1];
    const dir = await resolveDir(parts.slice(0, -1), { create: false });
    try {
        const file = await (await dir.getFileHandle(name)).getFile();
        return makeStats('file', file.size, file.lastModified);
    } catch { /* not a file */ }
    try {
        await dir.getDirectoryHandle(name);
        return makeStats('directory', 0);
    } catch { /* not a directory */ }
    throw fsError('ENOENT', `ENOENT: no such file or directory, stat '${path}'`);
}

/** @param {import('fs').PathLike} path */
export const lstat = stat;

/**
 * @param {import('fs').PathLike} path
 * @param {{ recursive?: boolean, force?: boolean }} [options]
 */
export async function rm(path, options = {}) {
    const { dir, name } = await resolveParent(path, { create: false }).catch((error) => {
        if (options.force) return { dir: null, name: '' };
        throw error;
    });
    if (!dir) return;
    try {
        await dir.removeEntry(name, { recursive: !!options.recursive });
    } catch {
        if (!options.force)
            throw fsError('ENOENT', `ENOENT: no such file or directory, rm '${path}'`);
    }
}

/** @param {import('fs').PathLike} path */
export async function unlink(path) {
    const { dir, name } = await resolveParent(path, { create: false });
    try {
        await dir.removeEntry(name);
    } catch {
        throw fsError('ENOENT', `ENOENT: no such file or directory, unlink '${path}'`);
    }
}

/** @param {import('fs').PathLike} path */
export async function rmdir(path) {
    return rm(path, { recursive: true });
}

/**
 * OPFS has no native move, so rename copies then deletes. Sufficient for FYLO's
 * write-temp-then-rename atomic replace pattern.
 * @param {import('fs').PathLike} oldPath
 * @param {import('fs').PathLike} newPath
 */
export async function rename(oldPath, newPath) {
    const info = await stat(oldPath);
    if (info.isDirectory()) {
        await copyDirectory(oldPath, newPath);
        await rm(oldPath, { recursive: true });
        return;
    }
    const bytes = await readFile(oldPath);
    await writeFile(newPath, bytes);
    await unlink(oldPath);
}

/**
 * OPFS has no hard links; emulate with a content copy. Adequate for FYLO's
 * write-temp + link + unlink durability pattern.
 * @param {import('fs').PathLike} existingPath
 * @param {import('fs').PathLike} newPath
 */
export async function link(existingPath, newPath) {
    const bytes = await readFile(existingPath);
    await writeFile(newPath, bytes);
}

/** OPFS has no POSIX permissions. */
export async function chmod() { /* no-op */ }
/** OPFS has no settable timestamps. */
export async function utimes() { /* no-op */ }

/**
 * @param {import('fs').PathLike} source
 * @param {import('fs').PathLike} destination
 */
async function copyDirectory(source, destination) {
    await mkdir(destination, { recursive: true });
    for (const entry of await readdir(source, { withFileTypes: true })) {
        const item = /** @type {{ name: string, isDirectory(): boolean }} */ (entry);
        const from = `${source}/${item.name}`;
        const to = `${destination}/${item.name}`;
        if (item.isDirectory())
            await copyDirectory(from, to);
        else
            await writeFile(to, await readFile(from));
    }
}

export default { open, mkdir, readdir, readFile, writeFile, stat, lstat, rm, rmdir, unlink, rename, link, chmod, utimes };
