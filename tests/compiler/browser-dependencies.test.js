import { describe, expect, test } from 'bun:test'
import Compiler from '../../src/compiler/index.js'

describe('browser dependency discovery', () => {
  test('normalizes bare and scoped package specifiers', () => {
    expect(Compiler.packageNameFromSpecifier('dayjs/plugin/utc')).toBe('dayjs')
    expect(Compiler.packageNameFromSpecifier('@scope/browser-client/dist/index.js')).toBe('@scope/browser-client')
    expect(Compiler.packageNameFromSpecifier('./local.js')).toBe('')
    expect(Compiler.packageNameFromSpecifier('/shared/scripts/imports.js')).toBe('')
  })

  test('finds browser package imports without including relative modules', () => {
    const imports = Compiler.importedPackageNames(`
      import dayjs from 'dayjs'
      import utc from 'dayjs/plugin/utc'
      export { helper } from '@scope/toolkit/runtime'
      const mod = await import('@scope/browser-client')
      import './component.js'
    `)

    expect(imports.sort()).toEqual(['@scope/browser-client', '@scope/toolkit', 'dayjs'])
  })
})
