type InventoryItem = {
    id: string
    name: string
    source: string
    createdAt: string
}

type FyloDocEntry = { id: string, doc: Record<string, unknown> }

export default class {
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
        const parsed = new Date(value)
        return Number.isNaN(parsed.getTime()) ? 'just now' : this.dateFormatter.format(parsed)
    }

    submitItem(event: Event): void {
        event.preventDefault()
        void this.addItem()
    }

    @subscribe('tachyon:refresh', { onMount: true })
    async refresh(): Promise<void> {
        try {
            // The injected fylo client reads the OPFS-backed local mirror —
            // this collection lives entirely in your browser.
            const result = await fylo['atlas-items'].find({}) as { docs?: FyloDocEntry[] }
            this.items = (result.docs ?? [])
                .map((entry) => ({ id: entry.id, ...(entry.doc as Omit<InventoryItem, 'id'>) }))
                .sort((a, b) => (b.createdAt ?? '').localeCompare(a.createdAt ?? ''))
        } catch {
            this.items = []
        }
    }

    async addItem(): Promise<void> {
        const name = this.newItem.trim()
        if (!name) {
            this.feedback = 'Name is required before saving.'
            return
        }

        const created = await fylo['atlas-items'].create({
            name,
            source: 'atlas',
            createdAt: new Date().toISOString(),
        }) as { ok?: boolean, id?: string, error?: string }

        if (!created.ok) {
            this.feedback = created.error ?? 'Failed to store the item.'
            return
        }

        await this.refresh()
        this.newItem = ''
        this.feedback = `Stored "${name}" in the browser-local FYLO mirror (${created.id}).`
        this.notifyInventoryChanged()
    }

    async clearItems(): Promise<void> {
        for (const item of this.items) {
            await fylo['atlas-items'].del(item.id)
        }
        this.items = []
        this.feedback = 'Inventory reset — documents removed from the OPFS mirror.'
        this.notifyInventoryChanged()
    }

    @publish('inventory:changed')
    notifyInventoryChanged(): { count: number } {
        return { count: this.items.length }
    }
}
