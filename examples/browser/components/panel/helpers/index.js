// @ts-check

export default class extends Tac {
    /** @type {string} */
    $draftNote = 'TACHYON ships pure JavaScript + strict JSDoc across Tac and Yon.'

    /** @returns {void} */
    clearDraft() {
        this.$draftNote = ''
    }

    /** @returns {void} */
    toggleTheme() {
        window.dispatchEvent(new CustomEvent('tachyon:toggle-theme'))
    }
}
