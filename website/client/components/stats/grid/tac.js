// @ts-check

export default class {
    /** @type {number} */
    $visits = 0
    /** @type {number} */
    $$totalVisits = 0
    /** @type {number} */
    itemsCount = 0
    /** @type {string} */
    companionStatus = 'starting'
    /** @type {string} */
    storageUsage = '—'
    /** @type {HTMLElement | null} */
    root = null

    constructor() {
        try {
            this.$visits = Number(sessionStorage.getItem('tachyon-atlas-visits') ?? 0) + 1
            this.$$totalVisits = Number(localStorage.getItem('tachyon-atlas-total-visits') ?? 0) + 1
            sessionStorage.setItem('tachyon-atlas-visits', String(this.$visits))
            localStorage.setItem('tachyon-atlas-total-visits', String(this.$$totalVisits))
        } catch {
            this.$visits += 1
            this.$$totalVisits += 1
        }
    }

    /** @param {HTMLElement} root @param {AbortSignal} signal */
    async hydrate(root, signal) {
        this.root = root
        const refresh = () => { void this.refresh() }
        window.addEventListener('tachyon:refresh', refresh, { signal })
        await this.refresh()
    }

    async refresh() {
        await Promise.all([
            this.loadItemsCount(),
            this.verifyCompanions(),
            this.measureStorage(),
        ])
        await this.root?.tachyonIsland?.refresh?.()
    }

    /** @returns {Promise<void>} */
    async loadItemsCount() {
        try {
            const stored = JSON.parse(localStorage.getItem('atlas-messages') ?? '[]')
            this.itemsCount = Array.isArray(stored) ? stored.length : 0
        } catch {
            this.itemsCount = 0
        }
    }

    /** @returns {Promise<void>} */
    async verifyCompanions() {
        this.companionStatus = 'companions ready'
    }

    /** @returns {Promise<void>} */
    async measureStorage() {
        try {
            const estimate = await navigator.storage?.estimate?.()
            this.storageUsage = estimate?.usage !== undefined
                ? `${(estimate.usage / 1024 / 1024).toFixed(1)} MB`
                : '—'
        } catch {
            this.storageUsage = '—'
        }
    }
}
