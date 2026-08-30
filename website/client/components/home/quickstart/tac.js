// @ts-nocheck -- validated by the Tachyon compiler and website contract gates.
export default class {
  /** Which step's copy button is showing feedback, and what it says. */
  copied = ''
  copiedLabel = 'Copied'

  /**
   * Called from an interpolation rather than an `on:` handler, so it takes its
   * argument directly — only a dispatched handler is passed the event first.
   */
  label(number) {
    return this.copied === number ? this.copiedLabel : 'Copy'
  }

  async copy(_event, number) {
    const step = this.steps().find((entry) => entry.number === number)
    if (!step) return
    this.copied = number
    try {
      await navigator.clipboard.writeText(step.command)
      this.copiedLabel = 'Copied'
    } catch {
      // A denied clipboard is the reader's choice rather than a fault.
      this.copiedLabel = 'Press \u2318C'
    }
    await globalThis.__tachyonTac?.render()
    setTimeout(() => {
      this.copied = ''
      void globalThis.__tachyonTac?.render()
    }, 2000)
  }

  steps() {
    return [
      {
        number: '01',
        title: 'Install',
        command: 'curl -fsSL https://tachyon.del.ma/install.sh | sh',
        note: 'One binary. No runtime to install first, and no daemon left behind.',
      },
      {
        number: '02',
        title: 'Scaffold',
        command: 'ty init my-app && cd my-app',
        note: 'A page, a component and a route, laid out the way the file-system router reads them.',
      },
      {
        number: '03',
        title: 'Serve',
        command: 'ty preview',
        note: 'Compiles on change and reloads the view without losing the state it was holding.',
      },
    ]
  }
}
