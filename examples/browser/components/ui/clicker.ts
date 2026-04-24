export default class extends Tac {
    $clicks: number | null = null
    label: string = 'Nested Clicker'
    release: string = 'Tac'

    constructor(props: Record<string, unknown> = {}) {
        super(props)
        this.label = String(this.props.label ?? 'Nested Clicker')
        this.$clicks = Number(this.props.clicks ?? 0)
        this.release = String(this.inject('demo-release', 'Tac'))
    }
}
