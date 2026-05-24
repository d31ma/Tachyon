// @ts-check
export default class extends Tac {
    /** @type {number} */
    $clicks = 0
    /** @type {string} */
    label = 'Interactions'
    /** @type {string | undefined} */
    @inject('demo-release', 'Tac')
    release
}
