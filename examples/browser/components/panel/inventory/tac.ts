import dayjs from 'dayjs'

type InventoryItem = {
    id: string
    name: string
    source: string
    createdAt: string
}

export default class extends Tac {
    items: InventoryItem[] = []
    newItem = ''
    feedback = ''
    dateFormatter = new Intl.DateTimeFormat('en-CA', {
        month: 'short',
        day: 'numeric',
        year: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
    })

    formatDate(value: string | null | undefined): string {
        if (!value) return 'just now'
        return dayjs(value).isValid()
            ? this.dateFormatter.format(dayjs(value).toDate())
            : 'just now'
    }

    @onMount
    async refresh(): Promise<void> {
        try {
            const response = await fetch('/languages/typescript/items', { cache: 'reload' })
            const payload = await response.json() as { items?: InventoryItem[] }
            this.items = payload.items ?? []
        } catch {
            this.items = []
        }
    }

    @onMount
    bindRefreshListener(): void {
        const handler = () => { this.refresh() }
        window.addEventListener('tachyon:refresh', handler)
    }

    async addItem(): Promise<void> {
        const name = this.newItem.trim()
        if (!name) {
            this.feedback = 'Name is required before saving.'
            return
        }

        const response = await fetch('/languages/typescript/items', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ name, source: 'dashboard' }),
        })

        const payload = response.status === 204
            ? { name }
            : await response.json() as { detail?: string } & Partial<InventoryItem>
        if (!response.ok) {
            this.feedback = payload.detail ?? 'Failed to create item.'
            return
        }

        await this.refresh()
        this.newItem = ''
        this.feedback = `Saved "${name}" through Yon.`
        window.dispatchEvent(new CustomEvent('inventory:changed'))
    }

    async clearItems(): Promise<void> {
        await fetch('/languages/typescript/items', { method: 'DELETE' })
        this.items = []
        this.feedback = 'Inventory reset.'
        window.dispatchEvent(new CustomEvent('inventory:changed'))
    }
}
