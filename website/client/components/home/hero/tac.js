// @ts-nocheck -- validated by the Tachyon compiler and website contract gates.
export default class {
  install = 'curl -fsSL https://tachyon.del.ma/install.sh | sh'
  copied = false

  label() {
    return this.copied ? 'copied' : 'copy'
  }

  async copy() {
    // A denied clipboard is the user's choice, not a failure to report. The
    // render is explicit because these assignments land after the await, past
    // the point where dispatching the event would have rendered for us.
    try {
      await navigator.clipboard.writeText(this.install)
      this.copied = true
    } catch {
      this.copied = false
    }
    await this.tac.render()
    setTimeout(() => {
      this.copied = false
      void this.tac.render()
    }, 2000)
  }
}
