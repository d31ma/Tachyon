import { afterEach, expect, test } from 'bun:test'
import { mkdtemp, mkdir, readFile, rm } from 'node:fs/promises'
import path from 'node:path'
import { tmpdir } from 'node:os'
import { createAppScaffold } from '../../src/runtime/app-scaffold.js'

const tempDirs: string[] = []

afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })))
})

test('createAppScaffold writes a deployable starter app', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-init-'))
    const appDir = path.join(root, 'my-app')
    tempDirs.push(root)

    const created = await createAppScaffold(appDir)
    expect(created).toBe(appDir)

    const packageJson = await readFile(path.join(appDir, 'package.json'), 'utf8')
    const homePage = await readFile(path.join(appDir, 'routes', 'HTML'), 'utf8')
    const layout = await readFile(path.join(appDir, 'routes', 'LAYOUT'), 'utf8')
    const amplify = await readFile(path.join(appDir, 'amplify.yml'), 'utf8')

    expect(packageJson).toContain('"preview": "tach.preview"')
    expect(packageJson).toContain('"@delma/tachyon"')
    expect(homePage).toContain('<hero />')
    expect(layout).toContain('<slot />')
    expect(amplify).toContain('baseDirectory: dist')
})

test('createAppScaffold refuses to overwrite a non-empty directory', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-init-'))
    const appDir = path.join(root, 'existing')
    tempDirs.push(root)

    await mkdir(appDir, { recursive: true })
    await Bun.write(path.join(appDir, 'README.md'), 'existing')

    await expect(createAppScaffold(appDir)).rejects.toThrow('is not empty')
})
