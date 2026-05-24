// @ts-check

/**
 * @typedef {{ title: string, summary: string }} ShowcasePayload
 */

export default class extends Tac {
    /** @type {ShowcasePayload | null} */
    showcase = null
    /** @type {boolean} */
    loading = true

    /** @returns {string} */
    loadingState() {
        return this.loading ? 'loading' : 'live'
    }

    @onMount
    async refresh() {
        this.loading = true
        try {
            const response = await fetch('/shared/data/showcase.json', { cache: 'reload' })
            this.showcase = /** @type {ShowcasePayload} */ (await response.json())
        } catch {
            this.showcase = null
        } finally {
            this.loading = false
        }
    }

    @onMount
    bindRefreshListener() {
        const handler = () => { this.refresh() }
        window.addEventListener('tachyon:refresh', handler)
    }
}
