import { spawnSync } from 'node:child_process'
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'

type RunResult = ReturnType<typeof spawnSync>

function run(command: string, args: string[], cwd: string) {
    return spawnSync(command, args, {
        cwd,
        env: process.env,
        encoding: 'utf8',
    })
}

function assertRun(result: RunResult, label: string) {
    if (result.status === 0 && !result.error) return

    throw new Error([
        `${label} failed with status ${result.status}`,
        result.error ? `error: ${result.error.message}` : undefined,
        result.stdout ? `stdout:\n${result.stdout}` : undefined,
        result.stderr ? `stderr:\n${result.stderr}` : undefined,
    ].filter(Boolean).join('\n\n'))
}

function expect(condition: unknown, message: string): asserts condition {
    if (!condition) throw new Error(message)
}

const repoRoot = process.cwd()
const tempRoot = await mkdtemp(path.join(os.tmpdir(), 'tachyon-package-contract-'))
const tarballPath = path.join(tempRoot, 'tachyon.tgz')
const consumerRoot = path.join(tempRoot, 'consumer')
const starterRoot = path.join(consumerRoot, 'starter-app')

try {
    const repoPackage = JSON.parse(await readFile(path.join(repoRoot, 'package.json'), 'utf8')) as {
        name?: string
    }
    const packageName = repoPackage.name
    expect(typeof packageName === 'string' && packageName.length > 0, 'package.json name must be a string')

    assertRun(run('bun', ['pm', 'pack', '--filename', tarballPath, '--quiet'], repoRoot), 'bun pm pack')

    await mkdir(consumerRoot, { recursive: true })
    await writeFile(path.join(consumerRoot, 'package.json'), JSON.stringify({
        private: true,
        type: 'module',
    }, null, 2))

    assertRun(run('bun', ['add', tarballPath], consumerRoot), `bun add ${tarballPath}`)

    const tachInit = path.join(consumerRoot, 'node_modules', '.bin', 'tach.init')
    assertRun(run('bun', [tachInit, 'starter-app'], consumerRoot), 'bun tach.init starter-app')

    const starterPackage = JSON.parse(await readFile(path.join(starterRoot, 'package.json'), 'utf8')) as {
        scripts?: Record<string, string>
        devDependencies?: Record<string, string>
    }
    const envExample = await readFile(path.join(starterRoot, '.env.example'), 'utf8')
    const route = await readFile(path.join(starterRoot, 'routes', 'HTML'), 'utf8')

    expect(starterPackage.scripts?.bundle === 'tach.bundle', 'starter app bundle script should be tach.bundle')
    expect(starterPackage.scripts?.serve === 'tach.serve', 'starter app serve script should be tach.serve')
    expect(starterPackage.scripts?.preview?.includes('tach.preview'), 'starter app preview script should include tach.preview')
    expect(typeof starterPackage.devDependencies?.[packageName] === 'string', `starter app should depend on ${packageName}`)
    expect(starterPackage.devDependencies?.[packageName]?.startsWith('^'), `${packageName} version should be caret-ranged`)
    expect(envExample.includes('YON_FORMAT=esm'), '.env.example should include YON_FORMAT=esm')
    expect(route.includes('<hero />'), 'starter routes/HTML should include <hero />')

    console.log(`Verified packed package contract for ${packageName}`)
} finally {
    await rm(tempRoot, { recursive: true, force: true })
}
