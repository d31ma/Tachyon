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

export default class extends Tac {
    /** @type {string} */
    $sampleText = 'Tachyon'
    /** @type {string} */
    $range = '72'
    /** @type {string} */
    $accent = '#3f51b5'
    /** @type {number} */
    $interactionCount = 0
    /** @type {boolean} */
    $subscribed = false
    /** @type {string} */
    $plan = 'starter'
    /** @type {InputValues} */
    fields = {
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
    /** @type {string} */
    lastControl = 'Ready for input'
    /** @type {string} */
    lastValue = 'Choose any control below to observe its value.'

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

    /**
     * @param {boolean} checked
     * @returns {void}
     */
    updateSubscription(checked) {
        this.$subscribed = checked
        this.record('checkbox', checked ? 'checked' : 'unchecked')
    }

    /** @param {string} value @returns {void} */
    updatePlan(value) {
        this.$plan = value
        this.record('radio', value)
    }

    /** @param {FileList | null} files @returns {void} */
    recordFiles(files) {
        const count = files?.length ?? 0
        this.record('file', count === 1 ? files?.[0]?.name : `${count} files`)
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
        this.$accent = '#3f51b5'
        this.$subscribed = false
        this.$plan = 'starter'
        this.fields = {
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
        this.record('reset', 'restored demonstration values')
    }
}
