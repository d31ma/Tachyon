// @ts-check

/**
 * @typedef {{
 *   search: string,
 *   email: string,
 *   password: string,
 *   telephone: string,
 *   link: string,
 *   number: string,
 *   date: string,
 *   dateTime: string,
 *   month: string,
 *   week: string,
 *   time: string,
 *   stack: string,
 *   runtime: string,
 *   notes: string
 * }} InputValues
 */

const DEFAULT_FIELDS = {
  search: '',
  email: '',
  password: '',
  telephone: '',
  link: '',
  number: '12',
  date: '',
  dateTime: '',
  month: '',
  week: '',
  time: '',
  stack: 'frontend',
  runtime: '',
  notes: 'Reactive components feel natural in Tac.',
}

export default class {
  /** @type {string} */
  $sampleText = 'Tachyon'
  /** @type {string} */
  $range = '72'
  /** @type {string} */
  $accent = '#0ea5e9'
  /** @type {number} */
  $interactionCount = 0
  /** @type {boolean} */
  $subscribed = false
  /** @type {string} */
  $plan = 'starter'
  /** @type {boolean} */
  $dragOver = false
  /** @type {string} */
  $fileName = ''
  /** @type {string} */
  $fileSize = ''
  /** @type {InputValues} */
  fields = { ...DEFAULT_FIELDS }
  /** @type {string} */
  lastControl = 'Ready for input'
  /** @type {string} */
  lastValue = 'Choose any control to observe its value.'

  /**
   * @param {string} control
   * @param {unknown} value
   * @returns {void}
   */
  record(control, value) {
    this.lastControl = control
    this.lastValue = String(value || '(empty)')
    this.$interactionCount += 1
  }

  /** @param {string} value @returns {void} */
  updateText(value) {
    this.$sampleText = value
    this.record('text', value)
  }

  /** @param {string} value @returns {void} */
  updateRange(value) {
    this.$range = value
    this.record('range', value)
  }

  /** @param {string} value @returns {void} */
  updateColour(value) {
    this.$accent = value
    this.record('color', value)
  }

  /**
   * @param {keyof InputValues} field
   * @param {string} value
   * @returns {void}
   */
  updateField(field, value) {
    this.fields = { ...this.fields, [field]: value }
    this.record(field === 'link' ? 'url' : field, value)
  }

  /** @param {string} value @returns {void} */
  recordSecret(value) {
    this.fields = { ...this.fields, password: value }
    this.record('password', value ? `${value.length} concealed characters` : '(empty)')
  }

  /** @param {boolean} checked @returns {void} */
  updateSubscription(checked) {
    this.$subscribed = checked
    this.record('checkbox', checked ? 'checked' : 'unchecked')
  }

  /** @param {string} value @returns {void} */
  updatePlan(value) {
    this.$plan = value
    this.record('radio', value)
  }

  /** @param {number} bytes @returns {string} */
  formatSize(bytes) {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  /** @param {FileList | null} files @returns {void} */
  recordFiles(files) {
    const count = files?.length ?? 0
    this.$dragOver = false
    if (count === 1) {
      this.$fileName = files?.[0]?.name ?? ''
      this.$fileSize = this.formatSize(files?.[0]?.size ?? 0)
      this.record('file', files?.[0]?.name)
    } else if (count > 1) {
      this.$fileName = `${count} files selected`
      this.$fileSize = this.formatSize(Array.from(files ?? []).reduce((sum, file) => sum + file.size, 0))
      this.record('file', `${count} files`)
    } else {
      this.$fileName = ''
      this.$fileSize = ''
      this.record('file', '(none)')
    }
  }

  /** @param {DragEvent} event @returns {void} */
  onDragOver(event) {
    event.preventDefault()
    this.$dragOver = true
  }

  /** @returns {void} */
  onDragLeave() {
    this.$dragOver = false
  }

  /** @param {DragEvent} event @returns {void} */
  onDrop(event) {
    event.preventDefault()
    this.recordFiles(event.dataTransfer?.files ?? null)
  }

  /** @returns {void} */
  triggerFilePicker() {
    const input = document.querySelector('#input-gallery input[type="file"]')
    if (input instanceof HTMLInputElement) input.click()
  }

  /** @param {Event} event @returns {void} */
  clearFile(event) {
    event.stopPropagation()
    this.$fileName = ''
    this.$fileSize = ''
    this.$dragOver = false
    const input = document.querySelector('#input-gallery input[type="file"]')
    if (input instanceof HTMLInputElement) input.value = ''
    this.record('file', 'cleared')
  }

  /**
   * @param {string} action
   * @param {Event} event
   * @returns {void}
   */
  preventAction(action, event) {
    event.preventDefault()
    this.record(action, 'prevented navigation')
  }

  /** @returns {void} */
  resetValues() {
    this.$sampleText = 'Tachyon'
    this.$range = '72'
    this.$accent = '#0ea5e9'
    this.$subscribed = false
    this.$plan = 'starter'
    this.fields = { ...DEFAULT_FIELDS }
    this.record('reset', 'restored demonstration values')
  }
}
