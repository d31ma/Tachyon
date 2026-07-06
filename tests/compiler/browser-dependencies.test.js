import { describe, expect, test } from 'bun:test'
import Compiler from '../../src/compiler/index.js'

describe('browser dependency discovery', () => {
  test('normalizes bare and scoped package specifiers', () => {
    expect(Compiler.packageNameFromSpecifier('dayjs/plugin/utc')).toBe('dayjs')
    expect(Compiler.packageNameFromSpecifier('@d31ma/fylo/dist/index.js')).toBe('@d31ma/fylo')
    expect(Compiler.packageNameFromSpecifier('./local.js')).toBe('')
    expect(Compiler.packageNameFromSpecifier('/shared/scripts/imports.js')).toBe('')
    expect(Compiler.packageNameFromSpecifier('tac://language')).toBe('')
  })

  test('finds browser package imports without including relative modules', () => {
    const imports = Compiler.importedPackageNames(`
      import dayjs from 'dayjs'
      import utc from 'dayjs/plugin/utc'
      export { helper } from '@scope/toolkit/runtime'
      const mod = await import('@d31ma/fylo')
      import './component.js'
    `)

    expect(imports.sort()).toEqual(['@d31ma/fylo', '@scope/toolkit', 'dayjs'])
  })
})
