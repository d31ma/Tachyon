import { describe, expect, test } from 'bun:test'
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

/** @typedef {{name: string, code: string}} Snippet */
/** @typedef {{heading: string, axis?: string, body?: string, files: Snippet[]}} Section */
/** @typedef {{order: string[], groups: {topics: string[]}[], topics: Record<string, {sections: Section[]}>}} Documentation */

class PublishedData {
  static languages = ['cs', 'dart', 'go', 'groovy', 'java', 'js', 'kt', 'php', 'py', 'rs', 'ts']
  static withRuby = ['cs', 'dart', 'fs', 'go', 'groovy', 'java', 'js', 'kt', 'lua', 'php', 'py', 'rb', 'rs', 'ts']
  static withoutLua = PublishedData.withRuby.filter(language => language !== 'lua')
  static withCpp = [...PublishedData.withRuby, 'hpp'].sort()
  /** @type {Record<string, string[]>} */
  static sectionLanguages = {
    'Annotated controllers and providers': PublishedData.withCpp,
    'Checking values and method arguments': PublishedData.withoutLua,
    'Limited Store and Query': PublishedData.withRuby,
    'Reading settings and choosing configuration': PublishedData.withoutLua,
    'Calling a method in another language': PublishedData.withoutLua,
  }

  /** @param {string} name @returns {Promise<Documentation>} */
  static async load(name) {
    return JSON.parse(await readFile(new URL(`../client/shared/data/${name}.json`, import.meta.url), 'utf8'))
  }

  /** @param {unknown} value @returns {Snippet[]} */
  static files(value) {
    /** @type {Snippet[]} */
    const files = []
    if (!value || typeof value !== 'object') return files
    if ('name' in value && typeof value.name === 'string' && 'code' in value && typeof value.code === 'string') {
      files.push({ name: value.name, code: value.code })
    }
    for (const child of Object.values(value)) files.push(...this.files(child))
    return files
  }
}

describe('annotation routes and clients in published examples', () => {
  test('Go examples use standalone comment annotations without changing Rust attributes', async () => {
    let markedGo = 0
    let markedRust = 0
    for (const name of ['docs', 'features', 'languages']) {
      for (const file of PublishedData.files(await PublishedData.load(name))) {
        if (file.name.endsWith('.rs') && file.code.includes('#[Service]')) markedRust += 1
        if (!file.name.endsWith('.go')) continue
        expect(file.code).not.toContain('#[')
        for (const line of file.code.split('\n')) {
          if (!line.includes('// @')) continue
          expect(line).toMatch(/^\s*\/\/ @[A-Za-z]/)
          markedGo += 1
        }
      }
    }
    expect(markedGo).toBeGreaterThan(100)
    expect(markedRust).toBeGreaterThan(10)
  })

  test('Ruby examples write each annotation as its own comment line', async () => {
    let markedRuby = 0
    for (const name of ['docs', 'features', 'languages']) {
      for (const file of PublishedData.files(await PublishedData.load(name))) {
        if (!file.name.endsWith('.rb') || file.name.startsWith('server/helpers/')) continue
        for (const line of file.code.split('\n')) {
          expect(line).not.toMatch(/^\s*@[A-Z]/)
          expect(line).not.toMatch(/^\s*# \[[A-Z]/)
          if (!line.trimStart().startsWith('#[')) continue
          expect(line).toMatch(/^\s*#\[[A-Z][A-Za-z]*(\(.*\))?\]$/)
          markedRuby += 1
        }
      }
    }
    expect(markedRuby).toBeGreaterThan(40)
  })

  test('every source example uses current entry paths and annotation names', async () => {
    for (const name of ['docs', 'features', 'languages']) {
      const files = PublishedData.files(await PublishedData.load(name))
      for (const file of files) {
        expect(file.name).not.toMatch(/^server\/routes\/|\/yon\.[a-z]+$|OPTIONS\.schema\.json$/)
        if (!/\.(js|ts|py|php|java|kt|cs|rs|dart|go|swift|rb|lua)$/.test(file.name)) continue
        expect(file.code).not.toMatch(/(?:@|#\[|\[)(?:Client|Controller|WebCall|HTTPExchange|HttpExchange|GetExchange|RequestMapping|GetMapping)\b/)
      }
    }
  })

  test('complete Yon guide examples include all ten supported languages', async () => {
    const docs = await PublishedData.load('docs')
    for (const heading of ['Annotated controllers and providers', 'Limited Store and Query', 'Reading settings and choosing configuration', 'Checking values and method arguments', 'Calling a method in another language']) {
      const section = Object.values(docs.topics).flatMap(topic => topic.sections).find(section => section.heading === heading)
      assert.ok(section)
      const controllers = section.files.filter(file => file.name.startsWith('server/controllers/'))
      const languages = PublishedData.sectionLanguages[heading] ?? PublishedData.languages
      expect(controllers.map(file => file.name.split('.').pop()).sort()).toEqual(languages)
      expect(section.axis).toBe('language')
    }
  })

  test('the linked glossary explains every routing and client annotation', async () => {
    const docs = await PublishedData.load('docs')
    expect(docs.order).toContain('routing-annotations')
    expect(docs.groups.some(group => group.topics.includes('routing-annotations'))).toBe(true)
    const glossary = docs.topics['routing-annotations']
    const text = [...glossary.sections, ...docs.topics.sockets.sections].map(section => section.body).join('\n')
    for (const name of ['RequestRoute', 'GetRoute', 'PostRoute', 'PutRoute', 'PatchRoute', 'DeleteRoute', 'ResponseStatus', 'FromPath', 'FromQuery', 'FromBody', 'FromHeader', 'FromCookie', 'HTTPClient', 'GetRequest', 'PostRequest', 'PutRequest', 'PatchRequest', 'DeleteRequest', 'PathVariable', 'RequestParam', 'RequestBody', 'RequestHeader', 'SocketRoute', 'OnOpen', 'OnMessage', 'OnClose', 'OnError', 'SocketRequest']) {
      expect(text).toContain(name)
    }
    const examples = glossary.sections.find(section => section.axis === 'language')
    assert.ok(examples)
    const controllers = examples.files.filter(file => file.name.startsWith('server/controllers/'))
    expect(controllers.map(file => file.name.split('.').pop()).sort()).toEqual(PublishedData.withCpp)
  })

  test('compiled HTTP examples declare a client contract that the service builds', async () => {
    const docs = await PublishedData.load('docs')
    const section = docs.topics.annotations.sections.find(section => section.heading === 'Outgoing HTTP: Tac')
    assert.ok(section)
    for (const extension of ['cs', 'dart', 'go', 'java', 'kt', 'rs', 'swift']) {
      /** @type {{ name: string, code: string } | undefined} */
      const service = section.files.find(file => file.name === `client/services/CatalogService.${extension}`)
      /** @type {{ name: string, code: string } | undefined} */
      const client = section.files.find(file => file.name === `client/remotes/CatalogClient.${extension}`)
      assert.ok(service)
      assert.ok(client)
      expect(service.code).toMatch(/(?:@|#\[|\[)Service\b/)
      expect(service.code).not.toMatch(/(?:@|#\[|\[)HTTPClient\b/)
      expect(client.code).toMatch(/(?:@|#\[|\[)HTTPClient\b/)
      expect(client.code).toMatch(/(?:@|#\[|\[)GetRequest\b/)
      expect(client.code).toMatch(/\b(interface|trait|protocol)\b/)
    }
  })
})
