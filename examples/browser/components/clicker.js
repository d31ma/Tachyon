// @ts-check
export default class extends Tac {
    /** @type {number | null} */
    $clicks = null
    /** @type {string} */
    label = 'Interactions'
    /** @type {string} */
    release = 'Tac'

    /**
     * @param {Record<string, unknown>} [props]
     */
    constructor(props = {}) {
        super(props)
        this.label = String(this.props.label ?? 'Interactions')
        this.$clicks = Number(this.props.clicks ?? 0)
        this.release = String(this.inject('demo-release', 'Tac'))
    }
}
