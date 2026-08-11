export default class HotCounter {
  count = 1
  version = 'one'

  hydrate(root, signal) {
    root.dataset.moduleVersion = this.version
    signal.addEventListener('abort', () => {
      sessionStorage.setItem('tachyon-hot-aborted', 'true')
    }, { once: true })
  }

  increment() {
    this.count += 1
  }

  hotState() {
    return { count: this.count }
  }

  restoreHotState(state) {
    this.count = state.count
  }

  hotDispose() {
    sessionStorage.setItem('tachyon-hot-disposed', 'true')
  }
}
