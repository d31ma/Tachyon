// @ts-check

export default class extends Tac {
    /** @type {string} */
    $draftNote = 'TACHYON ships pure JavaScript + strict JSDoc across Tac and Yon.'
    /** @type {number} */
    $$bookmarks = 0

    get charCount() {
        return this.$draftNote.length
    }

    get wordCount() {
        const trimmed = this.$draftNote.trim()
        return trimmed ? trimmed.split(/\s+/).length : 0
    }

    get draftStatus() {
        if (!this.$draftNote.trim()) return 'Empty'
        if (this.charCount < 20) return 'Short'
        return 'Ready'
    }

    /** @returns {void} */
    clearDraft() {
        this.$draftNote = ''
    }

    /** @returns {void} */
    @publish('tachyon:toggle-theme')
    toggleTheme() {
    }

    /** @returns {{ bookmarks: number }} */
    @publish('tac:bookmark')
    addBookmark() {
        this.$$bookmarks += 1
        return { bookmarks: this.$$bookmarks }
    }

    /** @returns {void} */
    resetBookmarks() {
        this.$$bookmarks = 0
    }
}
