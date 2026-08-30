import '../shared/scripts/imports.js'

export default class {
  static __tachyonOnMount = ['title']

  /**
   * Whether this page is being served by `ty preview`.
   *
   * The development server marks every document it serves; `ty bundle` never
   * does, so a published bundle cannot claim to be in development however it
   * is hosted.
   */
  development = document.body.dataset.tachyonEnvironment === 'development'

  /**
   * Who answered this page, shown only while developing.
   *
   * The site's own companion is JavaScript, so it says the same thing on every
   * target — which is the point worth seeing while working on it: a macOS,
   * Windows or Android build runs this module, not a rewrite of it.
   */
  runtime = 'JavaScript, in the browser'

  title() {
    document.title = 'Tachyon — one project, web and native'
  }
}
