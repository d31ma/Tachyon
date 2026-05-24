import { randomBytes } from 'node:crypto'
import { access, chmod, copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { mkdtempSync, rmSync } from 'node:fs'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

const repoRoot = path.resolve(fileURLToPath(new URL('../..', import.meta.url)))
const packageTempRoot = mkdtempSync(path.join(os.tmpdir(), 'tachyon-package-'))
let packedTarball

process.on('exit', () => {
  rmSync(packageTempRoot, { recursive: true, force: true })
})

export function uniqueName(prefix = 'tachyon') {
  return `${prefix}-${Date.now()}-${randomBytes(3).toString('hex')}`
}

export function run(command, args, { cwd, env = {}, timeout = 120_000 } = {}) {
  const result = spawnSync(command, args, {
    cwd,
    env: { ...process.env, ...env },
    encoding: 'utf8',
    timeout,
  })

  return {
    status: result.status,
    stdout: result.stdout ?? '',
    stderr: result.stderr ?? '',
    error: result.error,
  }
}

export function assertRun(result, label) {
  if (result.status === 0 && !result.error) return

  throw new Error(
    [
      `${label} failed with status ${result.status}`,
      result.error ? `error: ${result.error.message}` : undefined,
      result.stdout ? `stdout:\n${result.stdout}` : undefined,
      result.stderr ? `stderr:\n${result.stderr}` : undefined,
    ]
      .filter(Boolean)
      .join('\n\n')
  )
}

async function removeWithRetry(root, attempts = 5) {
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      await rm(root, { recursive: true, force: true })
      return
    } catch (error) {
      if (!['EBUSY', 'EPERM', 'ENOTEMPTY'].includes(error?.code)) throw error
      if (attempt === attempts) return
      await new Promise((resolve) => setTimeout(resolve, 150 * attempt))
    }
  }
}

export function tachyonTarball() {
  if (process.env.TACHYON_PACKAGE_TARBALL) return process.env.TACHYON_PACKAGE_TARBALL
  if (packedTarball) return packedTarball

  const tarball = path.join(packageTempRoot, `${uniqueName('tachyon-package')}.tgz`)
  const pack = run('bun', ['pm', 'pack', '--filename', tarball, '--quiet'], {
    cwd: repoRoot,
  })
  assertRun(pack, `bun pm pack --filename ${tarball}`)
  packedTarball = tarball
  return packedTarball
}

async function copyIfExists(source, destination) {
  try {
    await access(source)
  } catch (error) {
    if (error?.code === 'ENOENT') return
    throw error
  }
  await copyFile(source, destination)
}

export async function createTachyonApp() {
  const tarball = tachyonTarball()
  const root = await mkdtemp(path.join(os.tmpdir(), 'tachyon-app-'))

  await writeFile(
    path.join(root, 'package.json'),
    JSON.stringify(
      {
        private: true,
        type: 'module',
        scripts: {
          bundle: 'tac.bundle',
          serve: 'yon.serve',
          preview: 'tac.preview',
        },
      },
      null,
      2
    )
  )

  await copyIfExists(path.join(repoRoot, '.npmrc'), path.join(root, '.npmrc'))

  const install = run('bun', ['add', tarball], { cwd: root })
  assertRun(install, `bun add ${tarball}`)

  return {
    root,
    bin(name) {
      return path.join(root, 'node_modules', '.bin', name)
    },
    runBin(name, args = [], options = {}) {
      const result = run('bun', [this.bin(name), ...args], {
        cwd: options.cwd ?? root,
        env: options.env,
        timeout: options.timeout,
      })
      assertRun(result, `bun ${name} ${args.join(' ')}`.trim())
      return result
    },
    async writeFile(relativePath, content, mode) {
      const filePath = path.join(root, relativePath)
      await mkdir(path.dirname(filePath), { recursive: true })
      await writeFile(filePath, content)
      if (mode) await chmod(filePath, mode)
      return filePath
    },
    async readFile(relativePath) {
      return readFile(path.join(root, relativePath), 'utf8')
    },
    async cleanup() {
      await removeWithRetry(root)
    },
  }
}

export async function getFreePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer()
    server.once('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      if (!address || typeof address === 'string') {
        server.close()
        reject(new Error('Failed to resolve an ephemeral port'))
        return
      }
      server.close((error) => error ? reject(error) : resolve(address.port))
    })
  })
}

export async function waitFor(check, { timeoutMs = 20_000, intervalMs = 200 } = {}) {
  const startedAt = Date.now()
  while (Date.now() - startedAt < timeoutMs) {
    if (await check()) return true
    await Bun.sleep(intervalMs)
  }
  return false
}

export function startTachyon(app, { args = [], env = {} } = {}) {
  return Bun.spawn(['bun', app.bin('yon.serve'), ...args], {
    cwd: app.root,
    env: { ...process.env, ...env },
    stdout: 'pipe',
    stderr: 'pipe',
  })
}
