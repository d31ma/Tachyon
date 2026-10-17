import { describe, expect, test } from 'bun:test'
import { AnnotationReach } from '../scripts/annotation-reach.mjs'
import { CoverageAudit } from '../scripts/coverage-audit.mjs'
import { CoverageTable } from '../scripts/coverage-table.mjs'
import { RuleTableParser } from '../scripts/rule-table-parser.mjs'
import { SnippetUnits } from '../scripts/snippet-units.mjs'

class CoverageGate {
  static failOnUnexplainedGap = false
}

class Samples {
  static ruleTable = `
    const TAC: &'static [&'static str] = &["js", "rs"];
    const YON: &'static [&'static str] = &["js", "py", "java"];
    const TAC_BEHAVIOR: &'static [&'static str] = &["js", "rs"];
    const YON_BEHAVIOR: &'static [&'static str] = &["js", "py", "java"];
    const YON_STREAM: &'static [&'static str] = &["js", "py"];
    const TAC_HELPER: &'static [&'static str] = &["js"];
    const TAC_STREAM: &'static [&'static str] = &["js", "ts"];
    const NOWHERE: &'static [&'static str] = &[];
    pub(crate) const ALL: &'static [Rule] = &[
        Rule::new(
            "Service",
            Self::TYPE,
            "marks a class (a role) that other classes use",
        )
        .bare(),
        Rule::new("Helper", Self::TYPE, "marks a helper")
        .reach(Self::TAC_HELPER, Self::YON),
        Rule::new("StreamRoute", Self::METHOD, "streams")
        .route()
        .reach(Self::NOWHERE, Self::YON_STREAM),
        Rule::new("StreamRequest", Self::METHOD, "receives a stream")
        .reach(Self::TAC_STREAM, Self::NOWHERE),
        Rule::retired("Component", "Helper", "write @Helper"),
    ];

    const ACCESS_RULES: &'static [Rule] = &[
        Rule::new("Public", Self::TYPE_OR_METHOD, "anyone may call")
        .access(),
    ];

    impl Rule {
        const fn route(self) -> Self {
            Self {
                tac: AnnotationRules::NOWHERE,
                yon: AnnotationRules::YON,
                code: 2015,
                ..self
            }
        }

        const fn access(self) -> Self {
            Self {
                tac: AnnotationRules::NOWHERE,
                yon: AnnotationRules::YON,
                code: 2015,
                ..self
            }
        }
    }
  `

  /** @returns {CoverageAudit} */
  static audit() {
    return new CoverageAudit({ tac: ['js', 'rs', 'fs'], yon: ['js', 'py', 'java'] }, AnnotationReach.parse(Samples.ruleTable))
  }

  /**
   * @param {string} title
   * @param {{ name: string, code?: string, named?: boolean }[]} files
   * @param {string} [text]
   * @returns {import('../scripts/snippet-units.mjs').SnippetUnit}
   */
  static unit(title, files, text = '') {
    return {
      page: 'docs',
      id: 'topic',
      title,
      text,
      files: files.map(file => ({ name: file.name, extension: file.name.split('.').pop() ?? '', code: file.code ?? '', named: file.named ?? true })),
    }
  }

  /**
   * @param {import('../scripts/coverage-audit.mjs').UnitCoverage} coverage
   * @param {string} realm
   * @returns {Record<string, string>}
   */
  static kinds(coverage, realm) {
    const found = coverage.realms.find(entry => entry.realm === realm)
    return Object.fromEntries((found?.gaps ?? []).map(gap => [gap.language, gap.kind]))
  }
}

describe('the annotation rule table parser', () => {
  test('reads the realm languages, presets, reach calls, codes and retired names', () => {
    const table = RuleTableParser.parse(Samples.ruleTable)
    expect(table.realmLanguages).toEqual({ tac: ['js', 'rs'], yon: ['js', 'py', 'java'] })
    expect(table.rules).toEqual([
      { name: 'Service', tac: ['js', 'rs'], yon: ['js', 'py', 'java'], code: 1308 },
      { name: 'Helper', tac: ['js'], yon: ['js', 'py', 'java'], code: 1308 },
      { name: 'StreamRoute', tac: [], yon: ['js', 'py'], code: 2015 },
      { name: 'StreamRequest', tac: ['js', 'ts'], yon: [], code: 1308 },
      { name: 'Component', tac: [], yon: [], code: 1308 },
      { name: 'Public', tac: [], yon: ['js', 'py', 'java'], code: 2015 },
    ])
  })

  test('reads the real rule table of the compiler', () => {
    const reach = AnnotationReach.load()
    expect(reach.rule('Helper')?.tac).toEqual(['js', 'ts', 'dart', 'rs', 'py', 'cs', 'fs', 'swift'])
    expect(reach.rule('Public')?.tac).toEqual([])
    expect(reach.rule('stream_route')?.yon).not.toContain('java')
    expect(reach.refusal('yon', 'java', ['StreamRoute'], ['js'])).toEqual({ annotation: 'StreamRoute', code: 'TY2015' })
    expect(reach.refusal('tac', 'java', ['StreamRequest'], ['js', 'ts'])).toEqual({ annotation: 'StreamRequest', code: 'TY1308' })
  })
})

describe('the language coverage audit', () => {
  test('a section states its refusals in its summary or its body', () => {
    const unit = SnippetUnits.sectionUnit('streams', { heading: 'Streaming: Yon', summary: 'Java stays single-response.', body: 'Stream many messages.' })
    expect(unit.text).toContain('Java stays single-response.')
    expect(unit.text).toContain('Stream many messages.')
  })

  test('an explicit Python runtime manifest is configuration rather than a programming language', () => {
    const page = { tac: { files: [
      { name: 'client/manifest.json' },
      { name: 'client/components/basket/tally/tac.html' },
      { name: 'client/services/TallyService.py' },
    ] }, yon: { entries: [] } }
    expect(CoverageAudit.siteLanguages(page)).toEqual({ tac: ['py'], yon: [] })
  })
  test('a language the build refuses is not supported yet, and the section may say so', () => {
    const stream = { name: 'server/controllers/TicksController.js', code: '@RequestRoute("/ticks")\n@Public\nclass TicksController {\n  @StreamRoute\n  streamTicks() {}\n}\n' }
    const quiet = Samples.audit().audit([Samples.unit('Streaming', [stream])])[0]
    const stated = Samples.audit().audit([Samples.unit('Streaming', [stream], 'Java stays single-response.')])[0]
    expect(Samples.kinds(quiet, 'yon')).toEqual({ py: CoverageAudit.kinds.missing, java: CoverageAudit.kinds.unsupported })
    expect(quiet.realms[0].gaps[1]).toMatchObject({ code: 'TY2015', stated: false })
    expect(stated.realms[0].gaps[1]).toMatchObject({ code: 'TY2015', stated: true })
  })

  test('an annotation that only the browser JavaScript holds refuses the languages the rule table does not check', () => {
    const client = { name: 'client/clients/TicksClient.js', code: '@HTTPClient("/ticks")\nexport default class TicksClient {\n  @StreamRequest\n  streamTicks(tick) {}\n}\n' }
    const coverage = Samples.audit().audit([Samples.unit('Streams', [client], 'Rust and F# refuse StreamRequest.')])[0]
    expect(Samples.kinds(coverage, 'tac')).toEqual({ rs: CoverageAudit.kinds.unsupported, fs: CoverageAudit.kinds.unsupported })
    expect(coverage.realms[0].gaps[1]).toMatchObject({ code: 'TY1308', stated: true })
  })

  test('an annotation that a shown language cannot use explains no gap', () => {
    const files = [
      { name: 'client/helpers/Format.js', code: '@Helper\nexport default class Format {}\n' },
      { name: 'client/services/Cart.rs', code: '#[Service]\nstruct Cart {}\n' },
    ]
    const coverage = Samples.audit().audit([Samples.unit('Roles', files)])[0]
    expect(coverage.realms[0].present).toEqual(['js', 'rs'])
    expect(Samples.kinds(coverage, 'tac')).toEqual({ fs: CoverageAudit.kinds.missing })
    expect(coverage.realms[0].gaps[0].reason).toBe(CoverageAudit.unknownToRuleTable)
  })

  test('a section whose heading names languages leaves the others to its topic', () => {
    const scoped = Samples.unit('Browser services: JavaScript', [{ name: 'client/services/Cart.js' }])
    const general = Samples.unit('Services', [{ name: 'client/services/Cart.rs' }])
    const [scopedCoverage, generalCoverage] = Samples.audit().audit([scoped, general])
    expect(Samples.kinds(scopedCoverage, 'tac')).toEqual({ rs: CoverageAudit.kinds.notApplicable, fs: CoverageAudit.kinds.notApplicable })
    expect(Samples.kinds(generalCoverage, 'tac')).toEqual({ js: CoverageAudit.kinds.missing, fs: CoverageAudit.kinds.missing })
  })

  test('a language the text says does not apply is not applicable only when the text names it', () => {
    const files = [{ name: 'client/services/Tally.rs', code: '#[Service]\nstruct Tally {}\n' }]
    const quiet = { ...Samples.unit('WebAssembly', files), notApplicable: ['js'] }
    const stated = { ...Samples.unit('WebAssembly', files, 'JavaScript is not WebAssembly.'), notApplicable: ['js'] }
    const [quietCoverage, statedCoverage] = Samples.audit().audit([quiet, stated])
    expect(Samples.kinds(quietCoverage, 'tac')).toEqual({ js: CoverageAudit.kinds.missing, fs: CoverageAudit.kinds.missing })
    expect(Samples.kinds(statedCoverage, 'tac')).toEqual({ js: CoverageAudit.kinds.notApplicable, fs: CoverageAudit.kinds.missing })
    expect(statedCoverage.realms[0].gaps[0]).toMatchObject({ reason: CoverageAudit.statedNotApplicable, stated: true })
  })

  test('model files count only when the realm shows no role file', () => {
    const files = [{ name: 'server/controllers/GuideController.py' }, { name: 'server/models/Guide.java' }]
    const coverage = Samples.audit().audit([Samples.unit('Models', files)])[0]
    expect(coverage.realms[0].present).toEqual(['py'])
  })

  test('a snippet without a path gets its realm from its code', () => {
    expect(CoverageAudit.realmOf({ name: 'example.py', extension: 'py', code: '@request_route("/a")\nclass A:\n', named: false })).toBe('yon')
    expect(CoverageAudit.realmOf({ name: 'example.js', extension: 'js', code: '@Service\nexport default class A {}\n', named: false })).toBe('tac')
    expect(CoverageAudit.realmOf({ name: 'example.js', extension: 'js', code: 'document.title = "a"\n', named: false })).toBeUndefined()
  })
})

describe('language coverage of the published site', () => {
  const audit = CoverageAudit.load()
  const coverages = audit.audit(SnippetUnits.collect())

  test('the realm languages come from the Languages page', () => {
    expect(audit.realmLanguages.tac).toEqual(expect.arrayContaining(['js', 'ts', 'rs', 'swift', 'fs', 'java']))
    expect(audit.realmLanguages.yon).toEqual(expect.arrayContaining(['js', 'py', 'php', 'groovy', 'fs', 'rb']))
  })

  test('every covered realm accounts for each of its languages once', () => {
    for (const coverage of coverages) {
      for (const realm of coverage.realms) {
        const accounted = [...realm.present, ...realm.gaps.map(gap => gap.language)].sort()
        expect(accounted).toEqual([...audit.realmLanguages[realm.realm]].sort())
      }
    }
  })

  test('a section that shows every Yon language has no gap', () => {
    const controllers = coverages.find(coverage => coverage.title === 'Annotated controllers and providers')
    expect(controllers?.realms.map(realm => [realm.realm, realm.gaps.length])).toEqual([['yon', 0]])
  })

  test.if(CoverageGate.failOnUnexplainedGap)('every missing language has an example or a stated reason', () => {
    expect(CoverageTable.unexplained(coverages)).toEqual([])
  })
})
