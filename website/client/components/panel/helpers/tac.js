// @ts-check

export default class {
  /** @type {string} */
  $draftNote = 'Tachyon ships pure JavaScript + strict JSDoc across Tac and Yon.'
  /** @type {number} */
  $$bookmarks = 0
  /** @type {HTMLElement | null} */
  root = null

  constructor() {
    try {
      this.$draftNote = sessionStorage.getItem('atlas-draft-note') ?? this.$draftNote
      this.$$bookmarks = Number(localStorage.getItem('atlas-bookmarks') ?? 0)
    } catch {
      // Storage may be disabled; the in-memory demonstration still works.
    }
  }

  /** @param {HTMLElement} root @returns {void} */
  hydrate(root) {
    this.root = root
    const textarea = root.querySelector('textarea')
    if (textarea instanceof HTMLTextAreaElement) textarea.value = this.$draftNote
  }

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
    try { sessionStorage.setItem('atlas-draft-note', value) } catch {}
  }

  /** @returns {void} */
  clearDraft() {
    this.$draftNote = ''
    const textarea = this.root?.querySelector('textarea')
    if (textarea instanceof HTMLTextAreaElement) textarea.value = ''
    try { sessionStorage.removeItem('atlas-draft-note') } catch {}
  }

  /** @returns {{ bookmarks: number }} */
  addBookmark() {
    this.$$bookmarks += 1
    try { localStorage.setItem('atlas-bookmarks', String(this.$$bookmarks)) } catch {}
    window.dispatchEvent(new CustomEvent('tac:bookmark', { detail: { bookmarks: this.$$bookmarks } }))
    return { bookmarks: this.$$bookmarks }
  }

  /** @returns {void} */
  resetBookmarks() {
    this.$$bookmarks = 0
    try { localStorage.removeItem('atlas-bookmarks') } catch {}
  }
}
