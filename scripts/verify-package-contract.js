// @ts-check
import { spawnSync } from 'child_process';
import { access, copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from 'fs/promises';
import os from 'os';
import path from 'path';

class PackageContractVerifier {
    constructor() {
        this.repoRoot = process.cwd();
        this.tempRoot = '';
        this.tarballPath = '';
        this.consumerRoot = '';
        this.starterRoot = '';
        this.githubPackagesToken = process.env.NODE_AUTH_TOKEN || process.env.GITHUB_TOKEN || '';
    }

    async init() {
        this.tempRoot = await mkdtemp(path.join(os.tmpdir(), 'tachyon-package-contract-'));
        this.tarballPath = path.join(this.tempRoot, 'tachyon.tgz');
        this.consumerRoot = path.join(this.tempRoot, 'consumer');
        this.starterRoot = path.join(this.consumerRoot, 'starter-app');
    }

    /**
     * @param {string} command
     * @param {string[]} args
     * @param {string} cwd
     */
    run(command, args, cwd) {
        return spawnSync(command, args, {
            cwd,
            env: process.env,
            encoding: 'utf8',
        });
    }

    /**
     * @param {import('child_process').SpawnSyncReturns<string>} result
     * @param {string} label
     */
    assertRun(result, label) {
        if (result.status === 0 && !result.error)
            return;
        throw new Error([
            `${label} failed with status ${result.status}`,
            result.error ? `error: ${result.error.message}` : undefined,
            result.stdout ? `stdout:\n${result.stdout}` : undefined,
            result.stderr ? `stderr:\n${result.stderr}` : undefined,
        ].filter(Boolean).join('\n\n'));
    }

    /**
     * @param {unknown} condition
     * @param {string} message
     */
    expect(condition, message) {
        if (!condition)
            throw new Error(message);
    }

    /** @param {string} filePath */
    async fileExists(filePath) {
        try {
            await access(filePath);
            return true;
        }
        catch {
            return false;
        }
    }

    async copyRegistryCredentials() {
        const npmrcPath = path.join(this.repoRoot, '.npmrc');
        const consumerNpmrcPath = path.join(this.consumerRoot, '.npmrc');
        if (await this.fileExists(npmrcPath)) {
            await copyFile(npmrcPath, consumerNpmrcPath);
        }
        if (!this.githubPackagesToken)
            return;

        const existingNpmrc = await this.fileExists(consumerNpmrcPath)
            ? await readFile(consumerNpmrcPath, 'utf8')
            : '';
        await writeFile(consumerNpmrcPath, [
            existingNpmrc.trimEnd(),
            existingNpmrc.includes('@d31ma:registry=') ? '' : '@d31ma:registry=https://npm.pkg.github.com',
            `//npm.pkg.github.com/:_authToken=${this.githubPackagesToken}`,
            'always-auth=true',
            '',
        ].filter(Boolean).join('\n'));
        await writeFile(path.join(this.consumerRoot, 'bunfig.toml'), [
            '[install.scopes]',
            'd31ma = { url = "https://npm.pkg.github.com", token = "$NODE_AUTH_TOKEN" }',
            '',
        ].join('\n'));
    }

    /** @param {string} packageName */
    async verifyStarterPackage(packageName) {
        const starterPackage = JSON.parse(await readFile(path.join(this.starterRoot, 'package.json'), 'utf8'));
        const envExample = await readFile(path.join(this.starterRoot, '.env.example'), 'utf8');
        const envTest = await readFile(path.join(this.starterRoot, '.env.test'), 'utf8');
        const route = await readFile(path.join(this.starterRoot, 'client', 'pages', 'tac.html'), 'utf8');
        const jsconfig = await readFile(path.join(this.starterRoot, 'jsconfig.json'), 'utf8');
        const tachyonEnv = await readFile(path.join(this.starterRoot, 'tachyon-env.d.ts'), 'utf8');

        this.expect(starterPackage.scripts?.bundle === 'tac.bundle', 'starter app bundle script should be tac.bundle');
        this.expect(starterPackage.scripts?.serve === 'yon.serve', 'starter app serve script should be yon.serve');
        this.expect(starterPackage.scripts?.preview?.includes('tac.preview'), 'starter app preview script should include tac.preview');
        this.expect(typeof starterPackage.devDependencies?.[packageName] === 'string', `starter app should depend on ${packageName}`);
        this.expect(starterPackage.devDependencies?.[packageName]?.startsWith('^'), `${packageName} version should be caret-ranged`);
        this.expect(envExample.includes('TAC_FORMAT=esm'), '.env.example should include TAC_FORMAT=esm');
        this.expect(envTest.includes('TAC_FORMAT=esm'), '.env.test should include TAC_FORMAT=esm');
        this.expect(route.includes('<hero />'), 'starter client/pages/tac.html should include <hero />');
        this.expect(jsconfig.includes('tachyon-env.d.ts'), 'starter jsconfig should include tachyon-env.d.ts');
        this.expect(tachyonEnv.includes('@d31ma/tachyon/globals'), 'starter env types should include Tachyon globals');
    }

    async verify() {
        await this.init();
        try {
            const repoPackage = JSON.parse(await readFile(path.join(this.repoRoot, 'package.json'), 'utf8'));
            const packageName = repoPackage.name;
            this.expect(typeof packageName === 'string' && packageName.length > 0, 'package.json name must be a string');
            this.assertRun(this.run('bun', ['pm', 'pack', '--filename', this.tarballPath, '--quiet'], this.repoRoot), 'bun pm pack');
            await mkdir(this.consumerRoot, { recursive: true });
            await this.copyRegistryCredentials();
            await writeFile(path.join(this.consumerRoot, 'package.json'), JSON.stringify({
                private: true,
                type: 'module',
            }, null, 2));
            this.assertRun(this.run('bun', ['add', this.tarballPath], this.consumerRoot), `bun add ${this.tarballPath}`);
            const yonInit = path.join(this.consumerRoot, 'node_modules', '.bin', 'yon.init');
            this.assertRun(this.run('bun', [yonInit, 'starter-app'], this.consumerRoot), 'bun yon.init starter-app');
            await this.verifyStarterPackage(packageName);
            const globalsCheck = this.run('bun', ['--eval', `import globals from '${packageName}/eslint-globals'; if (globals.Tac !== 'readonly') process.exit(1);`], this.starterRoot);
            this.assertRun(globalsCheck, `${packageName}/eslint-globals import`);
            console.log(`Verified packed package contract for ${packageName}`);
        }
        finally {
            await rm(this.tempRoot, { recursive: true, force: true });
        }
    }
}

await new PackageContractVerifier().verify();
