// @ts-nocheck
//
// In-memory Origin Private File System (OPFS) for tests. Implements the subset
// the OPFS fs VFS uses: FileSystemDirectoryHandle (getDirectoryHandle,
// getFileHandle, removeEntry, entries/keys/values) and FileSystemFileHandle
// (getFile, createWritable, createSyncAccessHandle with random-access I/O).

class FakeFile {
    constructor() {
        this.bytes = new Uint8Array(0);
        this.lastModified = Date.now();
    }
}

class FakeSyncAccessHandle {
    constructor(file) { this.file = file; }
    getSize() { return this.file.bytes.length; }
    read(target, opts = {}) {
        const at = opts.at ?? 0;
        const view = new Uint8Array(target.buffer, target.byteOffset, target.byteLength);
        const src = this.file.bytes.subarray(at, at + view.length);
        view.set(src);
        return src.length;
    }
    write(source, opts = {}) {
        const at = opts.at ?? 0;
        const src = new Uint8Array(source.buffer, source.byteOffset, source.byteLength);
        const end = at + src.length;
        if (end > this.file.bytes.length) {
            const grown = new Uint8Array(end);
            grown.set(this.file.bytes);
            this.file.bytes = grown;
        }
        this.file.bytes.set(src, at);
        this.file.lastModified = Date.now();
        return src.length;
    }
    truncate(size) {
        if (size < this.file.bytes.length) {
            this.file.bytes = this.file.bytes.slice(0, size);
        }
        else if (size > this.file.bytes.length) {
            const grown = new Uint8Array(size);
            grown.set(this.file.bytes);
            this.file.bytes = grown;
        }
    }
    flush() {}
    close() {}
}

class FakeFileHandle {
    constructor(file) { this.kind = 'file'; this.file = file; }
    async getFile() {
        const bytes = this.file.bytes;
        return {
            size: bytes.length,
            lastModified: this.file.lastModified,
            async arrayBuffer() { return bytes.slice().buffer; },
            async text() { return new TextDecoder().decode(bytes); },
        };
    }
    async createWritable() {
        const file = this.file;
        const chunks = [];
        return {
            async write(data) {
                chunks.push(data instanceof Uint8Array ? data : new TextEncoder().encode(String(data)));
            },
            async close() {
                const total = chunks.reduce((n, c) => n + c.length, 0);
                const out = new Uint8Array(total);
                let offset = 0;
                for (const chunk of chunks) { out.set(chunk, offset); offset += chunk.length; }
                file.bytes = out;
                file.lastModified = Date.now();
            },
        };
    }
    async createSyncAccessHandle() { return new FakeSyncAccessHandle(this.file); }
}

class FakeDirectoryHandle {
    constructor() {
        this.kind = 'directory';
        this.dirs = new Map();
        this.files = new Map();
    }

    async getDirectoryHandle(name, opts = {}) {
        if (this.dirs.has(name)) return this.dirs.get(name);
        if (this.files.has(name)) { const e = new Error(`'${name}' is a file`); e.name = 'TypeMismatchError'; throw e; }
        if (!opts.create) { const e = new Error(`'${name}' not found`); e.name = 'NotFoundError'; throw e; }
        const dir = new FakeDirectoryHandle();
        this.dirs.set(name, dir);
        return dir;
    }

    async getFileHandle(name, opts = {}) {
        if (this.files.has(name)) return new FakeFileHandle(this.files.get(name));
        if (this.dirs.has(name)) { const e = new Error(`'${name}' is a directory`); e.name = 'TypeMismatchError'; throw e; }
        if (!opts.create) { const e = new Error(`'${name}' not found`); e.name = 'NotFoundError'; throw e; }
        const file = new FakeFile();
        this.files.set(name, file);
        return new FakeFileHandle(file);
    }

    async removeEntry(name, opts = {}) {
        if (this.files.has(name)) { this.files.delete(name); return; }
        if (this.dirs.has(name)) {
            const dir = this.dirs.get(name);
            if (!opts.recursive && (dir.dirs.size || dir.files.size)) {
                const e = new Error(`'${name}' is not empty`); e.name = 'InvalidModificationError'; throw e;
            }
            this.dirs.delete(name);
            return;
        }
        const e = new Error(`'${name}' not found`); e.name = 'NotFoundError'; throw e;
    }

    async *entries() {
        for (const [name, file] of this.files) yield [name, new FakeFileHandle(file)];
        for (const [name, dir] of this.dirs) yield [name, dir];
    }

    async *keys() {
        for (const name of this.files.keys()) yield name;
        for (const name of this.dirs.keys()) yield name;
    }

    async *values() {
        for (const file of this.files.values()) yield new FakeFileHandle(file);
        for (const dir of this.dirs.values()) yield dir;
    }
}

/**
 * Install a fresh in-memory OPFS at `navigator.storage`. Returns the root
 * directory handle (inspect `.dirs` / `.files` to assert on-disk layout).
 */
export function installFakeOpfs() {
    const root = new FakeDirectoryHandle();
    Object.defineProperty(globalThis, 'navigator', {
        configurable: true,
        writable: true,
        value: { storage: { async getDirectory() { return root; } } },
    });
    return root;
}

export { FakeDirectoryHandle };
