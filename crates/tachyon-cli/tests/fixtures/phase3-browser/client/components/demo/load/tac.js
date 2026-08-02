export default class Load {
  constructor(props) {
    this.props = props
  }

  hydrate(root) {
    root.dataset.activated = this.props.label
  }
}
