// @ts-check
export default class {
    /** @type {number} */
    $clicks = 0
    /** @type {string} */
    label = 'Interactions'
    /** @param {{ clicks?: number, label?: string }} props */
    constructor(props = {}) {
        this.$clicks = Number(props.clicks ?? 0)
        this.label = String(props.label ?? 'Interactions')
    }

    /** @returns {void} */
    hydrate() {}
}
