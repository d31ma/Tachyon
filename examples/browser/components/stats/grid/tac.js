// @ts-check

export default class extends Tac {
    /** @type {number} */
    itemsCount = 0
    /** @type {{ health: string, ready: string, summary: string }} */
    health = { health: 'loading', ready: 'loading', summary: 'loading' }

    @onMount
    async refresh() {
        await Promise.all([
            this.loadItemsCount(),
            this.loadHealth(),
        ])
    }

    @onMount
    bindRefreshListeners() {
        const refreshHandler = () => { this.refresh() }
        window.addEventListener('tachyon:refresh', refreshHandler)
        const inventoryHandler = () => { this.loadItemsCount() }
        window.addEventListener('inventory:changed', inventoryHandler)
    }

    /** @returns {Promise<void>} */
    async loadItemsCount() {
        try {
            const response = await fetch('/languages/typescript/items', { cache: 'reload' })
            const payload = /** @type {{ items?: unknown[] }} */ (await response.json())
            this.itemsCount = payload.items?.length ?? 0
        } catch {
            this.itemsCount = 0
        }
    }

    /** @returns {Promise<void>} */
    async loadHealth() {
        try {
            const [healthRes, readyRes] = await Promise.all([
                fetch('/health', { cache: 'reload' }),
                fetch('/ready', { cache: 'reload' }),
            ])
            const [healthData, readyData] = /** @type {[{ status: string }, { status: string }]} */ (await Promise.all([
                healthRes.json(),
                readyRes.json(),
            ]))
            this.health = {
                health: healthData.status,
                ready: readyData.status,
                summary: `${healthData.status}/${readyData.status}`,
            }
        } catch {
            this.health = { health: 'error', ready: 'error', summary: 'error' }
        }
    }
}
