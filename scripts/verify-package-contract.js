// @ts-check
import { spawnSync } from 'child_process';
import { access, copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from 'fs/promises';
import os from 'os';
import path from 'path';
/**
 * @param {string} command
 * @param {string[]} args
 * @param {string} cwd
 */
function run(command, args, cwd) {
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
function assertRun(result, label) {
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
function expect(condition, message) {
    if (!condition)
        throw new Error(message);
}
const repoRoot = process.cwd();
const tempRoot = await mkdtemp(path.join(os.tmpdir(), 'tachyon-package-contract-'));
const tarballPath = path.join(tempRoot, 'tachyon.tgz');
const consumerRoot = path.join(tempRoot, 'consumer');
const starterRoot = path.join(consumerRoot, 'starter-app');
const githubPackagesToken = process.env.NODE_AUTH_TOKEN || process.env.GITHUB_TOKEN || '';
/** @param {string} filePath */
async function fileExists(filePath) {
    try {
        await access(filePath);
        return true;
    }
    catch {
        return false;
    }
}
try {
    const repoPackage = JSON.parse(await readFile(path.join(repoRoot, 'package.json'), 'utf8'));
    const packageName = repoPackage.name;
    expect(typeof packageName === 'string' && packageName.length > 0, 'package.json name must be a string');
    assertRun(run('bun', ['pm', 'pack', '--filename', tarballPath, '--quiet'], repoRoot), 'bun pm pack');
    await mkdir(consumerRoot, { recursive: true });
    const npmrcPath = path.join(repoRoot, '.npmrc');
    const consumerNpmrcPath = path.join(consumerRoot, '.npmrc');
    if (await fileExists(npmrcPath)) {
        await copyFile(npmrcPath, consumerNpmrcPath);
    }
    if (githubPackagesToken) {
        const existingNpmrc = await fileExists(consumerNpmrcPath)
            ? await readFile(consumerNpmrcPath, 'utf8')
            : '';
        await writeFile(consumerNpmrcPath, [
            existingNpmrc.trimEnd(),
            existingNpmrc.includes('@d31ma:registry=') ? '' : '@d31ma:registry=https://npm.pkg.github.com',
            `//npm.pkg.github.com/:_authToken=${githubPackagesToken}`,
            'always-auth=true',
            '',
        ].filter(Boolean).join('\n'));
        await writeFile(path.join(consumerRoot, 'bunfig.toml'), [
            '[install.scopes]',
            'd31ma = { url = "https://npm.pkg.github.com", token = "$NODE_AUTH_TOKEN" }',
            '',
        ].join('\n'));
    }
    await writeFile(path.join(consumerRoot, 'package.json'), JSON.stringify({
        private: true,
        type: 'module',
    }, null, 2));
    assertRun(run('bun', ['add', tarballPath], consumerRoot), `bun add ${tarballPath}`);
    const yonInit = path.join(consumerRoot, 'node_modules', '.bin', 'yon.init');
    assertRun(run('bun', [yonInit, 'starter-app'], consumerRoot), 'bun yon.init starter-app');
    const starterPackage = JSON.parse(await readFile(path.join(starterRoot, 'package.json'), 'utf8'));
    const envExample = await readFile(path.join(starterRoot, '.env.example'), 'utf8');
    const envTest = await readFile(path.join(starterRoot, '.env.test'), 'utf8');
    const route = await readFile(path.join(starterRoot, 'browser', 'pages', 'index.html'), 'utf8');
    expect(starterPackage.scripts?.bundle === 'tac.bundle', 'starter app bundle script should be tac.bundle');
    expect(starterPackage.scripts?.serve === 'yon.serve', 'starter app serve script should be yon.serve');
    expect(starterPackage.scripts?.preview?.includes('tac.preview'), 'starter app preview script should include tac.preview');
    expect(typeof starterPackage.devDependencies?.[packageName] === 'string', `starter app should depend on ${packageName}`);
    expect(starterPackage.devDependencies?.[packageName]?.startsWith('^'), `${packageName} version should be caret-ranged`);
    expect(envExample.includes('TAC_FORMAT=esm'), '.env.example should include TAC_FORMAT=esm');
    expect(envTest.includes('TAC_FORMAT=esm'), '.env.test should include TAC_FORMAT=esm');
    expect(route.includes('<hero />'), 'starter browser/pages/index.html should include <hero />');
    console.log(`Verified packed package contract for ${packageName}`);
}
finally {
    await rm(tempRoot, { recursive: true, force: true });
}
