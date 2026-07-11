// @ts-check
/**
 * Provisioning for the Dart compiler used by Tac companions. The SDK is kept
 * outside application source and verified before extraction, so authors do not
 * need a global Dart installation or a project-level configuration file.
 */

import { access, mkdir, open, rename, rm, stat, unlink, writeFile } from 'fs/promises';
import path from 'path';
import TachyonRuntimeCache from '../shared/runtime-cache.js';

const DART_VERSION = '3.6.0';
const LOCK_TIMEOUT_MS = 120_000;
const STALE_LOCK_MS = 15 * 60_000;

/** @type {Record<string, { archive: string, sha256: string }>} */
const RELEASES = {
    'darwin-arm64': { archive: 'dartsdk-macos-arm64-release.zip', sha256: '1bdbc6544aaa53673e7cbbf66ad7cde914cb7598936ebbd6a4245e1945a702a0' },
    'darwin-x64': { archive: 'dartsdk-macos-x64-release.zip', sha256: 'b859b1abd92997b389061be6b301e598a3edcbf7e092cfe5b8d6ce2acdf0732b' },
    'linux-arm64': { archive: 'dartsdk-linux-arm64-release.zip', sha256: '0f82f10f808c7003d0d03294ae9220b5e0824ab3d2d19b4929d4fa735254e7bf' },
    'linux-x64': { archive: 'dartsdk-linux-x64-release.zip', sha256: '8e14ff436e1eec72618dabc94f421a97251f2068c9cc9ad2d3bb9d232d6155a3' },
    'win32-arm64': { archive: 'dartsdk-windows-arm64-release.zip', sha256: '93f406ccd8eba563ea20d734f09d8121d3d9d45c5d05b63385d72b30ca7184f7' },
    'win32-x64': { archive: 'dartsdk-windows-x64-release.zip', sha256: 'be7e6bec6ee131a2fc55612d98af61793f3944457fc6825e72bb2d5abb7dd8ad' },
};

/** @param {string} filePath */
async function exists(filePath) {
    try {
        await access(filePath);
        return true;
    }
    catch {
        return false;
    }
}

/** @param {string} value */
function quotePowerShell(value) {
    return `'${value.replaceAll("'", "''")}'`;
}

export default class DartToolchain {
    /** @param {string} [cacheRoot] */
    constructor(cacheRoot = TachyonRuntimeCache.cacheRoot()) {
        this.cacheRoot = cacheRoot;
    }

    /** @returns {{ archive: string, sha256: string, key: string }} */
    release() {
        const key = `${process.platform}-${process.arch}`;
        const release = RELEASES[key];
        if (!release) {
            throw new Error(`Tachyon cannot provision Dart for ${key}. Install Dart and set TACHYON_DART_COMPILER instead.`);
        }
        return { ...release, key };
    }

    /** @returns {string} */
    root() {
        return path.join(this.cacheRoot, 'toolchains', 'dart', DART_VERSION, this.release().key);
    }

    /** @returns {string} */
    commandPath() {
        return path.join(this.root(), 'dart-sdk', 'bin', process.platform === 'win32' ? 'dart.exe' : 'dart');
    }

    /** @returns {string} */
    downloadUrl() {
        return `https://storage.googleapis.com/dart-archive/channels/stable/release/${DART_VERSION}/sdk/${this.release().archive}`;
    }

    /** @returns {Promise<string>} */
    async ensure() {
        const command = this.commandPath();
        if (await exists(command))
            return command;

        const root = this.root();
        await mkdir(path.dirname(root), { recursive: true });
        const lockPath = `${root}.lock`;
        const lock = await this.acquireLock(lockPath, command);
        if (!lock)
            return command;

        const stagingRoot = `${root}.staging-${process.pid}-${Bun.randomUUIDv7()}`;
        const archivePath = path.join(path.dirname(root), `${this.release().archive}.${Bun.randomUUIDv7()}`);
        try {
            if (await exists(command))
                return command;
            const response = await fetch(this.downloadUrl());
            if (!response.ok)
                throw new Error(`Tachyon could not download the Dart SDK (${response.status} ${response.statusText}).`);
            const bytes = new Uint8Array(await response.arrayBuffer());
            const hash = new Bun.CryptoHasher('sha256').update(bytes).digest('hex');
            if (hash !== this.release().sha256)
                throw new Error('Tachyon rejected the downloaded Dart SDK because its SHA-256 checksum did not match the pinned release.');
            await writeFile(archivePath, bytes);
            await mkdir(stagingRoot, { recursive: true });
            await this.extract(archivePath, stagingRoot);
            const stagedCommand = path.join(stagingRoot, 'dart-sdk', 'bin', process.platform === 'win32' ? 'dart.exe' : 'dart');
            if (!await exists(stagedCommand))
                throw new Error('Tachyon could not find the Dart executable after extracting the verified SDK.');
            await rm(root, { recursive: true, force: true });
            await rename(stagingRoot, root);
            return command;
        }
        finally {
            await rm(stagingRoot, { recursive: true, force: true });
            await rm(archivePath, { force: true });
            await lock.close();
            await rm(lockPath, { force: true });
        }
    }

    /** @param {string} lockPath @param {string} command */
    async acquireLock(lockPath, command) {
        const deadline = Date.now() + LOCK_TIMEOUT_MS;
        while (Date.now() < deadline) {
            try {
                return await open(lockPath, 'wx');
            }
            catch (error) {
                if (!(error instanceof Error) || !('code' in error) || error.code !== 'EEXIST')
                    throw error;
                if (await exists(command))
                    return null;
                try {
                    if (Date.now() - (await stat(lockPath)).mtimeMs > STALE_LOCK_MS)
                        await unlink(lockPath);
                }
                catch {
                    // Another Tachyon process may have completed the install.
                }
                await Bun.sleep(50);
            }
        }
        throw new Error(`Timed out while provisioning the Dart SDK at ${this.root()}.`);
    }

    /** @param {string} archivePath @param {string} destination */
    async extract(archivePath, destination) {
        const command = process.platform === 'win32'
            ? ['powershell', '-NoProfile', '-NonInteractive', '-Command', `Expand-Archive -LiteralPath ${quotePowerShell(archivePath)} -DestinationPath ${quotePowerShell(destination)} -Force`]
            : ['unzip', '-q', archivePath, '-d', destination];
        const proc = Bun.spawn({ cmd: command, stdout: 'pipe', stderr: 'pipe' });
        const [stdout, stderr, exitCode] = await Promise.all([
            new Response(proc.stdout).text(),
            new Response(proc.stderr).text(),
            proc.exited,
        ]);
        if (exitCode !== 0)
            throw new Error(`Tachyon could not extract the Dart SDK:\n${stderr || stdout}`.trim());
    }
}

export { DART_VERSION };
