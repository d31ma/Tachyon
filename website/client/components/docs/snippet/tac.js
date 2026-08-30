// @ts-nocheck -- validated by the Tachyon compiler and website contract gates.
import {
  PLATFORMS,
  PLATFORM_CHANGED,
  filesFor,
  inReadingOrder,
  languageOf,
  isCompanion,
  rememberPlatform,
  spansPlatforms,
  storedPlatform,
} from '/shared/scripts/platforms.js'

export default class {
  static __tachyonOnMount = ['follow']

  /**
   * `files` arrives as a property and is deliberately not declared here: a
   * property is only assigned when the instance does not already carry that
   * name, so declaring it would shadow what the template passes in.
   *
   * @type {{ name: string, code: string }[] | undefined}
   */

  /**
   * `axis` arrives as a property. `language` drops the platform strip: a Yon
   * handler has no client target to be read as, so the only axis it has is the
   * language it is written in.
   *
   * @type {string | undefined}
   */

  platform = ''
  /** Which companion is on show, by language. Empty means the first. */
  companion = ''
  /** Which file's copy button is showing feedback, if any. */
  copied = ''
  /** What that feedback says: confirmed, or what to press instead. */
  copiedLabel = 'Copied'

  list() {
    return Array.isArray(this.files) ? this.files : []
  }

  spans() {
    return this.axis !== 'language' && spansPlatforms(this.list())
  }

  /** The platform strip, with the current one marked. */
  platforms() {
    const here = this.platform || storedPlatform()
    return PLATFORMS.map((platform) => ({ ...platform, current: platform.id === here }))
  }

  /** Every file this example has for the chosen platform, in reading order. */
  #available() {
    const files = this.spans() ? filesFor(this.list(), this.platform || storedPlatform()) : this.list()
    // An empty file is structure rather than an example — a Python package
    // marker has to exist for the suite to run and has nothing to read.
    return inReadingOrder(files.filter((file) => file.code.trim() !== ''))
  }

  /**
   * The companions among them, as a tab strip.
   *
   * A view and a stylesheet are not companions and never appear here: they are
   * the same file on every target, so tabbing between them would be a choice
   * with one correct answer.
   */
  companions() {
    const files = this.#available().filter((file) => isCompanion(file.name))
    // Alternatives only when they are different languages. A Tac example has
    // one companion per language and picking one is a real choice; a Yon
    // example has a controller, a service and a repository that are all the
    // same language and all part of the same thing — tabbing between those
    // would hide two thirds of the example behind a control that looks like a
    // language switch.
    const languages = [...new Set(files.map((file) => languageOf(file.name)))]
    if (languages.length < 2) return []
    const chosen = languages.includes(this.companion) ? this.companion : languages[0]
    return languages.map((language) => ({
      name: language,
      label: language,
      current: language === chosen,
    }))
  }

  /**
   * The sources to show: everything that is not a companion, plus the one
   * companion selected.
   *
   * A file that is not a companion at all — a view, a stylesheet, a schema —
   * is kept whatever the platform, because it is the same file everywhere.
   */
  shown() {
    const files = this.#available()
    const chosen = this.companions().find((entry) => entry.current)?.name
    return files
      // With no choice to make, every file is shown. With one, the files of
      // the chosen language are shown and the others' are not.
      .filter((file) => !chosen || !isCompanion(file.name) || languageOf(file.name) === chosen)
      .map((file) => ({ ...file, language: languageOf(file.name) ?? '' }))
  }

  pick(_event, name) {
    this.companion = name
  }

  /**
   * Called from an interpolation rather than an `on:` handler, so it takes its
   * argument directly — only a dispatched handler is passed the event first.
   */
  label(name) {
    return this.copied === name ? this.copiedLabel : 'Copy'
  }

  /**
   * Picking a platform applies to every example on the page, not just this
   * one: a reader comparing a page's examples wants them in one platform, and
   * re-picking per block is the tax that stops them doing it.
   */
  choose(_event, id) {
    rememberPlatform(id)
    dispatchEvent(new CustomEvent(PLATFORM_CHANGED, { detail: { platform: id } }))
  }

  follow() {
    this.platform = storedPlatform()
    addEventListener(PLATFORM_CHANGED, (event) => {
      this.platform = event.detail.platform
      // A companion chosen under the old platform may not reach the new one,
      // so the selection falls back to the first that does.
      this.companion = ''
      void globalThis.__tachyonTac?.render()
    })
    void globalThis.__tachyonTac?.render()
  }

  async copy(_event, name) {
    const file = this.shown().find((entry) => entry.name === name)
    if (!file) return
    this.copied = name
    try {
      await navigator.clipboard.writeText(file.code)
      this.copiedLabel = 'Copied'
    } catch {
      // A denied clipboard is the reader's choice rather than a fault, so the
      // button says what to press instead of failing silently — which is what
      // it did when the card was rebuilt and this branch stopped saying
      // anything at all.
      this.copiedLabel = 'Press ⌘C'
    }
    await globalThis.__tachyonTac?.render()
    setTimeout(() => {
      this.copied = ''
      void globalThis.__tachyonTac?.render()
    }, 2000)
  }
}
