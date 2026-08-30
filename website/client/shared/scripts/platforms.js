// Which target a companion file reaches.
//
// DuVay tabs each example across the platforms it can be skinned as, and the
// choice is page-wide and remembered: you pick your platform once and read a
// whole page in it. The same idea is worth more here, because Tachyon's
// examples differ across platforms for a real reason rather than a cosmetic
// one — a `tac.swift` and a `tac.kt` are different code reaching different
// SDKs, not the same component wearing a different skin.
//
// The mapping is the companion matrix in `crates/tachyon-core/src/project.rs`.
// It lives here as data because a page cannot ask the compiler at run time;
// `every_language_reads_the_same_class_the_same_way` is what keeps the two
// honest on the compiler's side.

/** The platforms an example can be read as, in the order they are shown. */
export const PLATFORMS = [
  { id: 'web', label: 'Web' },
  { id: 'macos', label: 'macOS' },
  { id: 'ios', label: 'iOS' },
  { id: 'android', label: 'Android' },
  { id: 'windows', label: 'Windows' },
  { id: 'linux', label: 'Linux' },
]

/** Which targets each companion extension reaches. */
const REACH = {
  js: ['web', 'macos', 'ios', 'android', 'windows', 'linux'],
  ts: ['web', 'macos', 'ios', 'android', 'windows', 'linux'],
  html: ['web', 'macos', 'ios', 'android', 'windows', 'linux'],
  css: ['web', 'macos', 'ios', 'android', 'windows', 'linux'],
  swift: ['macos', 'ios'],
  kt: ['android'],
  cs: ['windows'],
  rs: ['macos', 'windows', 'linux'],
}

/** The language a file is written in, for the label beside its name. */
const LANGUAGES = {
  html: 'View',
  css: 'Styles',
  js: 'JavaScript',
  ts: 'TypeScript',
  py: 'Python',
  java: 'Java',
  php: 'PHP',
  swift: 'Swift',
  kt: 'Kotlin',
  cs: 'C#',
  rs: 'Rust',
  // Not languages Yon runs — Yon runs the eight that can declare a layer.
  // They are here because a delegate is shown in them: a program Yon hands
  // work to still appears on the page, and an unlabelled block reads as an
  // oversight rather than as a deliberate boundary.
  rb: 'Ruby',
  go: 'Go',
}

const extension = (name) => String(name).split('.').pop()?.toLowerCase() ?? ''

/**
 * Whether a file is server-side, and so has no client target at all.
 *
 * Decided by path rather than extension, because the two halves share every
 * language: a `tac.kt` is an Android companion and a `yon.kt` is a Kotlin
 * handler, and only where they sit tells them apart. Without this a Yon
 * example written in eight languages grew a platform strip offering to read a
 * server handler as macOS.
 */
const isServer = (name) =>
  String(name).startsWith('server/') || String(name).startsWith('middleware.')

/** The targets one file reaches, or null when nothing constrains it. */
export const reachOf = (name) => (isServer(name) ? null : REACH[extension(name)] ?? null)

/**
 * Whether a file is a companion — the behaviour half of a route.
 *
 * A view and a stylesheet also carry a reach, because they are valid on every
 * target, so reach alone cannot tell them apart from a `tac.js`. What
 * distinguishes a companion is that choosing between two of them is a real
 * choice: one route has one companion, but it may have several written for
 * different targets.
 */
const COMPANIONS = new Set(['js', 'ts', 'swift', 'kt', 'cs', 'rs', 'py', 'java', 'php'])

export const isCompanion = (name) => COMPANIONS.has(extension(name))

/**
 * The order a Tac example reads in: behaviour, then structure, then
 * presentation — the companion, its view, and the styles over it.
 *
 * The companion leads because it is the half that differs. A view and a
 * stylesheet are the same file on every target, so the thing a reader came to
 * compare should not be below two files they have already read.
 *
 * Sorted here rather than trusted from the data, because the data is authored
 * by hand across forty-odd entries and "the companion is first" is not a
 * property forty hand-written lists keep on their own.
 *
 * JavaScript before TypeScript within the companions: it is the one that needs
 * no toolchain, so it is where a reader lands before choosing otherwise.
 */
const ORDER = ['js', 'ts', 'py', 'java', 'php', 'swift', 'kt', 'cs', 'rs', 'html', 'css']

export const rank = (name) => {
  const at = ORDER.indexOf(extension(name))
  // A pseudo-file — `emitted`, `generated`, a diagram — is the result of the
  // example rather than part of it, so it sorts last whatever it is called.
  return at === -1 ? ORDER.length : at
}

/** One example's files, in reading order, without disturbing ties. */
export const inReadingOrder = (files) =>
  files
    .map((file, index) => ({ file, index }))
    .sort((left, right) => rank(left.file.name) - rank(right.file.name) || left.index - right.index)
    .map((entry) => entry.file)

export const languageOf = (name) => LANGUAGES[extension(name)] ?? null

/**
 * Whether a set of files is worth showing a platform strip for.
 *
 * One companion language reaches every platform on its own, so a strip over a
 * single `tac.js` would be six tabs that all do the same thing.
 */
export const spansPlatforms = (files) => {
  const reaches = files.map((file) => reachOf(file.name)).filter(Boolean)
  return new Set(reaches.map((reach) => reach.join(','))).size > 1
}

/** The files that reach one platform, falling back to all of them. */
export const filesFor = (files, platform) => {
  const matching = files.filter((file) => {
    const reach = reachOf(file.name)
    return reach === null || reach.includes(platform)
  })
  return matching.length > 0 ? matching : files
}

const KEY = 'tachyon.platform'

/**
 * The chosen platform, remembered across pages.
 *
 * Defaults to the one the reader is on, because landing on the docs and seeing
 * your own platform is the most direct demonstration the site can give. An
 * explicit choice wins over the guess.
 */
export const storedPlatform = () => {
  try {
    const chosen = localStorage.getItem(KEY)
    if (chosen && PLATFORMS.some((platform) => platform.id === chosen)) return chosen
  } catch {
    // A blocked store is the reader's choice, not a failure to report.
  }
  return detectPlatform()
}

export const rememberPlatform = (platform) => {
  try {
    localStorage.setItem(KEY, platform)
  } catch {
    // As above: the selection still applies to this page.
  }
}

/** A guess from the user agent, used only until the reader picks one. */
export const detectPlatform = () => {
  const agent = navigator.userAgent
  if (/iPhone|iPad|iPod/.test(agent)) return 'ios'
  if (/Android/.test(agent)) return 'android'
  if (/Macintosh/.test(agent)) return 'macos'
  if (/Windows/.test(agent)) return 'windows'
  if (/Linux/.test(agent)) return 'linux'
  return 'web'
}

/** Broadcast so every example on the page follows one choice. */
export const PLATFORM_CHANGED = 'tachyon:platform'
