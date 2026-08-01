export default class Interactive {
  constructor(props) {
    this.props = props
  }

  hydrate(root) {
    root.dataset.activated = this.props.label
    root.addEventListener('pointerdown', () => {
      root.dataset.replayed = String(Number(root.dataset.replayed || '0') + 1)
    })
  }
}
