// @ts-check

export default class {
  /** @type {string} */
  $draftNote = 'Tachyon ships pure JavaScript + strict JSDoc across Tac and Yon.'
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
    if (!this.$draftNote.trim()) return 'empty'
    if (this.charCount < 20) return 'short'
    return 'ready'
  }

  /** @param {string} value @returns {void} */
  updateDraft(value) {
    this.$draftNote = value
  }

  /** @returns {void} */
  clearDraft() {
    this.$draftNote = ''
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
