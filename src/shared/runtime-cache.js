// @ts-check
import { mkdir, mkdtemp, open, readFile, readdir, rename, rm, stat, unlink, writeFile } from 'fs/promises';
import { homedir } from 'os';
import path from 'path';

const CACHE_LAYOUT_VERSION = 1;
const CACHE_LOCK_TIMEOUT_MS = 30_000;
const STALE_CACHE_LOCK_MS = 60_000;

/**
 * Stores source files embedded in a compiled `ty` binary where Bun can resolve
 * them during a runtime build. Build artifacts themselves remain in memory.
 */
export default class TachyonRuntimeCache {
    /** @returns {string} */
    static cacheRoot() {
        const override = process.env.TACHYON_CACHE_DIR?.trim();
        if (override)
            return path.resolve(override);
        const home = process.env.HOME || process.env.USERPROFILE || homedir();
        if (process.platform === 'darwin')
            return path.join(home, 'Library', 'Caches', 'Tachyon');
        if (process.platform === 'win32')
            return path.join(process.env.LOCALAPPDATA || path.join(home, 'AppData', 'Local'), 'Tachyon', 'Cache');
        return path.join(process.env.XDG_CACHE_HOME || path.join(home, '.cache'), 'tachyon');
    }

    /** @returns {string} */
    static runtimeRoot() {
        return path.join(TachyonRuntimeCache.cacheRoot(), 'runtime');
    }

    /** @param {string} source */
    static hash(source) {
        return Bun.hash(source).toString(16);
    }

    /**
     * @param {Array<{ path: string, source: string }>} files
     * @returns {Array<{ path: string, source: string, hash: string }>}
     */
    static normalizeFiles(files) {
        const seen = new Set();
        return files.map((file) => {
            const relativePath = path.normalize(file.path).replaceAll('\\', '/');
            if (!relativePath || path.isAbsolute(relativePath) || relativePath === '..' || relativePath.startsWith('../'))
                throw new Error(`Invalid Tachyon runtime cache path: ${file.path}`);
            if (typeof file.source !== 'string')
                throw new Error(`Tachyon runtime cache source must be text: ${relativePath}`);
            if (seen.has(relativePath))
                throw new Error(`Duplicate Tachyon runtime cache path: ${relativePath}`);
            seen.add(relativePath);
            return { path: relativePath, source: file.source, hash: TachyonRuntimeCache.hash(file.source) };
        }).sort((left, right) => left.path.localeCompare(right.path));
    }

    /** @param {Array<{ path: string, hash: string }>} files */
    static cacheKey(files) {
        return TachyonRuntimeCache.hash(JSON.stringify({ layout: CACHE_LAYOUT_VERSION, files }));
    }

    /**
     * @param {string} root
     * @param {string} key
     * @param {Array<{ path: string, hash: string }>} files
     */
    static async isValid(root, key, files) {
        try {
            const manifest = JSON.parse(await readFile(path.join(root, 'manifest.json'), 'utf8'));
            if (manifest.layout !== CACHE_LAYOUT_VERSION || manifest.key !== key || !Array.isArray(manifest.files))
                return false;
            if (JSON.stringify(manifest.files) !== JSON.stringify(files))
                return false;
            for (const file of files) {
                const source = await readFile(path.join(root, file.path), 'utf8');
                if (TachyonRuntimeCache.hash(source) !== file.hash)
                    return false;
            }
            return true;
        }
        catch {
            return false;
        }
    }

    /**
     * Ensures only one process repairs a content-addressed cache entry at once.
     * @param {string} lockPath
     * @param {string} targetRoot
     * @param {string} key
     * @param {Array<{ path: string, hash: string }>} files
     * @returns {Promise<import('fs/promises').FileHandle | null>}
     */
    static async acquireLock(lockPath, targetRoot, key, files) {
        const deadline = Date.now() + CACHE_LOCK_TIMEOUT_MS;
        while (Date.now() < deadline) {
            try {
                return await open(lockPath, 'wx');
            }
            catch (error) {
                if (!(error instanceof Error) || !('code' in error) || error.code !== 'EEXIST')
                    throw error;
                if (await TachyonRuntimeCache.isValid(targetRoot, key, files))
                    return null;
                try {
                    const lock = await stat(lockPath);
                    if (Date.now() - lock.mtimeMs > STALE_CACHE_LOCK_MS)
                        await unlink(lockPath);
                }
                catch {
                    // A competing process may have just released the lock.
                }
                await Bun.sleep(25);
            }
        }
        throw new Error(`Timed out waiting for Tachyon runtime cache: ${targetRoot}`);
    }

    /**
     * Materializes a validated, content-addressed framework source tree.
     * @param {Array<{ path: string, source: string }>} files
     * @returns {Promise<string>}
     */
    static async materialize(files) {
        const normalizedFiles = TachyonRuntimeCache.normalizeFiles(files);
        const manifestFiles = normalizedFiles.map(({ path, hash }) => ({ path, hash }));
        const key = TachyonRuntimeCache.cacheKey(manifestFiles);
        const runtimeRoot = TachyonRuntimeCache.runtimeRoot();
        const targetRoot = path.join(runtimeRoot, key);
        if (await TachyonRuntimeCache.isValid(targetRoot, key, manifestFiles))
            return targetRoot;

        await mkdir(runtimeRoot, { recursive: true });
        const lockPath = `${targetRoot}.lock`;
        const lock = await TachyonRuntimeCache.acquireLock(lockPath, targetRoot, key, manifestFiles);
        if (!lock)
            return targetRoot;
        let stagingRoot = '';
        try {
            if (await TachyonRuntimeCache.isValid(targetRoot, key, manifestFiles))
                return targetRoot;
            stagingRoot = await mkdtemp(path.join(runtimeRoot, '.staging-'));
            for (const file of normalizedFiles) {
                const outputPath = path.join(stagingRoot, file.path);
                await mkdir(path.dirname(outputPath), { recursive: true });
                await writeFile(outputPath, file.source);
            }
            await writeFile(path.join(stagingRoot, 'manifest.json'), JSON.stringify({
                layout: CACHE_LAYOUT_VERSION,
                key,
                files: manifestFiles,
            }, null, 2));

            if (await TachyonRuntimeCache.isValid(targetRoot, key, manifestFiles))
                return targetRoot;
            await rm(targetRoot, { recursive: true, force: true });
            await rename(stagingRoot, targetRoot);
            return targetRoot;
        }
        finally {
            if (stagingRoot)
                await rm(stagingRoot, { recursive: true, force: true });
            await lock.close();
            await unlink(lockPath).catch(() => {});
        }
    }

    /** @returns {Promise<{ root: string, runtimeRoot: string, entries: number }>} */
    static async status() {
        const runtimeRoot = TachyonRuntimeCache.runtimeRoot();
        try {
            const entries = await readdir(runtimeRoot, { withFileTypes: true });
            return {
                root: TachyonRuntimeCache.cacheRoot(),
                runtimeRoot,
                entries: entries.filter((entry) => entry.isDirectory() && !entry.name.startsWith('.')).length,
            };
        }
        catch {
            return { root: TachyonRuntimeCache.cacheRoot(), runtimeRoot, entries: 0 };
        }
    }

    /** Clears only content materialized by the standalone runtime. */
    static async clear() {
        await rm(TachyonRuntimeCache.runtimeRoot(), { recursive: true, force: true });
    }
}
