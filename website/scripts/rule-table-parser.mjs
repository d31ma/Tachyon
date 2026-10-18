/** @typedef {'tac' | 'yon'} Realm */
/** @typedef {{ name: string, tac: string[], yon: string[], code: number }} AnnotationRule */
/** @typedef {{ tac?: string, yon?: string, code?: string }} ReachSettings */
/** @typedef {{ realmLanguages: Record<Realm, string[]>, rules: AnnotationRule[] }} RuleTable */

export class RuleTableParser {
  static listConstant = /const (\w+): &'static \[&'static str\]\s*=\s*&\[([^\]]*)\]/g
  static listAlias = /const (\w+): &'static \[&'static str\]\s*=\s*Self::(\w+);/g
  static quoted = /"([^"]*)"/g
  static ruleList = /const \w+: &'static \[Rule\] = &\[([\s\S]*?)\n {4}\];/g
  static ruleStart = /Rule::(?=new\(|retired\()/
  static retiredRule = 'retired('
  static builderCall = /\.(\w+)\(([^()]*)\)/g
  static presetBuilder = /const fn (\w+)\(self\) -> Self \{\s*Self \{([\s\S]*?)\.\.self/g
  static presetTac = /tac: AnnotationRules::(\w+)/
  static presetYon = /yon: AnnotationRules::(\w+)/
  static presetCode = /code: (\d+)/
  static reachArguments = /Self::(\w+),\s*Self::(\w+)/
  static reachBuilder = 'reach'
  static codeBuilder = 'code'
  static realmLists = { tac: 'TAC', yon: 'YON' }
  static startingReach = { tac: 'TAC_BEHAVIOR', yon: 'YON_BEHAVIOR', code: '1308' }
  static retiredReach = { tac: 'NOWHERE', yon: 'NOWHERE', code: '1308' }

  /**
   * @param {string} source
   * @returns {RuleTable}
   */
  static parse(source) {
    const lists = RuleTableParser.listConstants(source)
    const presets = RuleTableParser.presets(source)
    const realmLanguages = {
      tac: RuleTableParser.listNamed(lists, RuleTableParser.realmLists.tac),
      yon: RuleTableParser.listNamed(lists, RuleTableParser.realmLists.yon),
    }
    const rules = RuleTableParser.ruleTexts(source).map(text => RuleTableParser.ruleFrom(text, lists, presets))
    return { realmLanguages, rules }
  }

  /**
   * @param {string} source
   * @returns {Map<string, string[]>}
   */
  static listConstants(source) {
    const lists = new Map()
    for (const match of source.matchAll(RuleTableParser.listConstant)) lists.set(match[1], RuleTableParser.quotedWords(match[2]))
    for (const match of source.matchAll(RuleTableParser.listAlias)) lists.set(match[1], lists.get(match[2]) ?? [])
    return lists
  }

  /**
   * @param {string} source
   * @returns {Map<string, ReachSettings>}
   */
  static presets(source) {
    const presets = new Map()
    for (const match of source.matchAll(RuleTableParser.presetBuilder)) presets.set(match[1], RuleTableParser.presetSettings(match[2]))
    return presets
  }

  /**
   * @param {string} body
   * @returns {ReachSettings}
   */
  static presetSettings(body) {
    const settings = {
      tac: body.match(RuleTableParser.presetTac)?.[1],
      yon: body.match(RuleTableParser.presetYon)?.[1],
      code: body.match(RuleTableParser.presetCode)?.[1],
    }
    return Object.fromEntries(Object.entries(settings).filter(([, value]) => value !== undefined))
  }

  /**
   * @param {string} text
   * @returns {string[]}
   */
  static quotedWords(text) {
    return [...text.matchAll(RuleTableParser.quoted)].map(match => match[1])
  }

  /**
   * @param {Map<string, string[]>} lists
   * @param {string} name
   * @returns {string[]}
   */
  static listNamed(lists, name) {
    const list = lists.get(name)
    if (!list) throw new Error(`annotation_rules.rs has no language list named ${name}`)
    return list
  }

  /**
   * @param {string} source
   * @returns {string[]}
   */
  static ruleTexts(source) {
    const lists = [...source.matchAll(RuleTableParser.ruleList)].map(match => match[1])
    if (lists.length === 0) throw new Error('annotation_rules.rs has no rule list')
    return lists.flatMap(list => list.split(RuleTableParser.ruleStart).slice(1))
  }

  /**
   * @param {string} text
   * @param {Map<string, string[]>} lists
   * @param {Map<string, ReachSettings>} presets
   * @returns {AnnotationRule}
   */
  static ruleFrom(text, lists, presets) {
    const reach = RuleTableParser.reachSettings(text, presets)
    return {
      name: RuleTableParser.quotedWords(text)[0],
      tac: RuleTableParser.listNamed(lists, String(reach.tac)),
      yon: RuleTableParser.listNamed(lists, String(reach.yon)),
      code: Number(reach.code),
    }
  }

  /**
   * @param {string} text
   * @param {Map<string, ReachSettings>} presets
   * @returns {ReachSettings}
   */
  static reachSettings(text, presets) {
    const retired = text.startsWith(RuleTableParser.retiredRule)
    const reach = { ...(retired ? RuleTableParser.retiredReach : RuleTableParser.startingReach) }
    for (const call of text.matchAll(RuleTableParser.builderCall)) Object.assign(reach, RuleTableParser.callSettings(call, presets))
    return reach
  }

  /**
   * @param {RegExpMatchArray} call
   * @param {Map<string, ReachSettings>} presets
   * @returns {ReachSettings}
   */
  static callSettings(call, presets) {
    const [, name, argumentText] = call
    if (name === RuleTableParser.reachBuilder) return RuleTableParser.reachOf(argumentText)
    if (name === RuleTableParser.codeBuilder) return { code: argumentText.trim() }
    return presets.get(name) ?? {}
  }

  /**
   * @param {string} argumentText
   * @returns {ReachSettings}
   */
  static reachOf(argumentText) {
    const match = argumentText.match(RuleTableParser.reachArguments)
    if (!match) throw new Error(`annotation_rules.rs has a reach call that names no lists: ${argumentText}`)
    return { tac: match[1], yon: match[2] }
  }
}
