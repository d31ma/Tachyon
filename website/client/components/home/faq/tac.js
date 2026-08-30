export default class {
  entries() {
    return [
      {
        question: 'Is this another JavaScript framework?',
        answer: 'No. A Tac view is standards-based HTML with control tags, and its companion may be JavaScript, TypeScript, or Rust, Swift, Kotlin or C# compiled by its own toolchain for the target it runs on. None of those are transpiled to JavaScript.',
      },
      {
        question: 'What do I have to install?',
        answer: 'The ty binary. It scaffolds, compiles, bundles and packages, and ty preview is the development server for both Tac and Yon. Toolchains are only needed for the languages you actually use, and ty doctor reports which ones the machine already has.',
      },
      {
        question: 'How does the same source become a native application?',
        answer: 'A native build packages the application\'s own web bundle and hosts it in the web view the platform already ships, so every target renders the same thing. What is genuinely native is the companion compiled for that platform: it is linked into the host process, so its own SDK — AppKit, the Android SDK, the .NET base class library — is simply there. A window or a tray is that SDK\'s, not something Tachyon puts a verb in front of.',
      },
      {
        question: 'What makes a route a route?',
        answer: 'Its position on disk. A client/pages directory becomes a page, a server/routes directory holding a yon.* file becomes a REST endpoint, and a leading underscore marks a dynamic segment. There is no route table to keep in sync.',
      },
      {
        question: 'Can a handler be written in a language you do not support?',
        answer: 'Yes. A handler is any executable that speaks the JSON handler protocol over stdio. JavaScript and Python have built-in adapters; everything else starts from the shebang on its own first line.',
      },
      {
        question: 'Is there a configuration file?',
        answer: 'A tac.config.js module, not a data file, so configuration can derive what a static document could only repeat. It is optional — a project with no config still builds.',
      },
    ]
  }
}
