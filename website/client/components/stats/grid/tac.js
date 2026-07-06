// @ts-check

export default class {
    /** @type {number} */
    $visits = 0
    /** @type {number} */
    $$totalVisits = 0
    /** @type {number} */
    itemsCount = 0
    /** @type {string} */
    workerStatus = 'starting'
    /** @type {string} */
    storageUsage = '—'

    constructor() {
        this.$visits += 1
        this.$$totalVisits += 1
    }

    @subscribe('tachyon:refresh', { onMount: true })
    async refresh() {
        await Promise.all([
            this.loadItemsCount(),
            this.pingWorker(),
            this.measureStorage(),
        ])
    }

    /** @returns {Promise<void>} */
    @subscribe('inventory:changed')
    async loadItemsCount() {
        try {
            const result = /** @type {{ docs?: unknown[] }} */ (await fylo['atlas-items'].find({}))
            this.itemsCount = result.docs?.length ?? 0
        } catch {
            this.itemsCount = 0
        }
    }

    /** @returns {Promise<void>} */
    async pingWorker() {
        try {
            const response = await fetch('tac://language/javascript', { cache: 'reload' })
            this.workerStatus = response.ok ? 'wasm online' : 'error'
        } catch {
            this.workerStatus = 'unavailable'
        }
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
