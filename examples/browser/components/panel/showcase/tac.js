// @ts-check

/**
 * @typedef {{ title: string, summary: string }} ShowcasePayload
 * @typedef {{ key: string, size: number, preview: string }} StorageEntry
 */

export default class extends Tac {
    /** @type {ShowcasePayload | null} */
    showcase = null
    /** @type {boolean} */
    loading = true
    /** @type {StorageEntry[]} */
    storageEntries = []
    /** @type {number} */
    totalStorageBytes = 0
    /** @type {number} */
    sessionCount = 0

    /** @returns {string} */
    loadingState() {
        return this.loading ? 'loading' : 'live'
    }

    get storageSummary() {
        if (this.totalStorageBytes < 1024) return `${this.totalStorageBytes} B`
        return `${(this.totalStorageBytes / 1024).toFixed(1)} KB`
    }

    /** @returns {Promise<string>} */
    async awaitedTemplateNote() {
        await Promise.resolve()
        return this.showcase
            ? `Resolved "${this.showcase.title}" from an awaited HTML expression.`
            : 'Tac HTML can await companion methods during render.'
    }

    /** @returns {void} */
    scanStorage() {
        /** @type {StorageEntry[]} */
        const entries = []
        let total = 0
        for (let i = 0; i < localStorage.length; i++) {
            const key = localStorage.key(i)
            if (!key) continue
            const value = localStorage.getItem(key) ?? ''
            const size = new Blob([value]).size
            total += size
            const preview = value.length > 60 ? value.slice(0, 60) + '…' : value
            entries.push({ key, size, preview })
        }
        entries.sort((a, b) => b.size - a.size)
        this.storageEntries = entries
        this.totalStorageBytes = total
        this.sessionCount = sessionStorage.length
    }

    @subscribe('tachyon:refresh', { onMount: true })
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
        this.scanStorage()
    }

}
