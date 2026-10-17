import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { RuleTableParser } from './rule-table-parser.mjs'

/** @typedef {import('./rule-table-parser.mjs').Realm} Realm */
/** @typedef {import('./rule-table-parser.mjs').AnnotationRule} AnnotationRule */
/** @typedef {{ annotation: string, code: string }} Refusal */

export class AnnotationReach {
  static ruleTable = fileURLToPath(new URL('../../crates/tachyon-core/src/annotation_rules.rs', import.meta.url))
  static codePrefix = 'TY'
  static browserHeld = ['js', 'ts', 'go', 'swift']

  /** @type {Record<Realm, string[]>} */
  realmLanguages
  /** @type {Map<string, AnnotationRule>} */
  rules

  /**
   * @param {Record<Realm, string[]>} realmLanguages
   * @param {AnnotationRule[]} rules
   */
  constructor(realmLanguages, rules) {
    this.realmLanguages = realmLanguages
    this.rules = new Map(rules.map(rule => [AnnotationReach.plainName(rule.name), rule]))
  }

  /** @returns {AnnotationReach} */
  static load() {
    return AnnotationReach.parse(readFileSync(AnnotationReach.ruleTable, 'utf8'))
  }

  /**
   * @param {string} source
   * @returns {AnnotationReach}
   */
  static parse(source) {
    const table = RuleTableParser.parse(source)
    return new AnnotationReach(table.realmLanguages, table.rules)
  }

  /**
   * @param {string} name
   * @returns {string}
   */
  static plainName(name) {
    return name.replaceAll('_', '').toLowerCase()
  }

  /**
   * @param {string} name
   * @returns {AnnotationRule | undefined}
   */
  rule(name) {
    return this.rules.get(AnnotationReach.plainName(name))
  }

  /**
   * @param {Realm} realm
   * @param {string} language
   * @param {string[]} annotations
   * @param {string[]} shownLanguages
   * @returns {Refusal | undefined}
   */
  refusal(realm, language, annotations, shownLanguages) {
    const needed = this.knownRules(annotations).filter(rule => this.acceptsEvery(rule, realm, shownLanguages))
    const refused = needed.find(rule => this.refuses(rule, realm, language))
    return refused && { annotation: refused.name, code: `${AnnotationReach.codePrefix}${refused.code}` }
  }

  /**
   * @param {AnnotationRule} rule
   * @param {Realm} realm
   * @param {string} language
   * @returns {boolean}
   */
  refuses(rule, realm, language) {
    if (rule[realm].includes(language)) return false
    if (this.realmLanguages[realm].includes(language)) return true
    return AnnotationReach.isBrowserHeldOnly(rule[realm])
  }

  /**
   * @param {string[]} languages
   * @returns {boolean}
   */
  static isBrowserHeldOnly(languages) {
    return languages.length > 0 && languages.every(language => AnnotationReach.browserHeld.includes(language))
  }

  /**
   * @param {AnnotationRule} rule
   * @param {Realm} realm
   * @param {string[]} languages
   * @returns {boolean}
   */
  acceptsEvery(rule, realm, languages) {
    const checked = languages.filter(language => this.realmLanguages[realm].includes(language))
    return checked.every(language => rule[realm].includes(language))
  }

  /**
   * @param {string[]} annotations
   * @returns {AnnotationRule[]}
   */
  knownRules(annotations) {
    return annotations.map(name => this.rule(name)).filter(rule => rule !== undefined)
  }
}
