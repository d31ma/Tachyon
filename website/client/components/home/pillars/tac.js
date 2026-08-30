export default class {
  /**
   * Six, because that is what fits a three-by-two grid without a widow, and
   * because a seventh claim is always the one nobody reads.
   */
  static PILLARS = [
    {
      title: 'One binary',
      body: 'The toolchain is a single executable. No runtime is installed beside your application, and none ships inside it.',
    },
    {
      title: 'Six targets',
      body: 'The browser, macOS, Windows, Linux, Android and iOS, from one project and one ty build --target.',
    },
    {
      title: 'File-system routed',
      body: 'A directory is a path. Pages live under client/pages, REST handlers under server/routes, and the route graph is the tree.',
    },
    {
      title: 'Layered handlers',
      body: 'A handler is a class that declares its layer. Yon runs the eight languages whose syntax can carry that declaration, and checks it against where the file sits.',
    },
    {
      title: 'Native companions',
      body: 'A page can be answered by Swift, Kotlin, C# or Rust, each compiled for its target and linked into the host — not bridged.',
    },
    {
      title: 'Standards first',
      body: 'Views are HTML with braces. No virtual DOM, no client router, no build-time dialect of a language its own compiler would reject.',
    },
  ]

  list() {
    return this.constructor.PILLARS
  }
}
