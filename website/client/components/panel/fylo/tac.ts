type DemoStep = { op: string, detail: string }

export default class {
    storageBackend = 'checking…'
    usageLabel = '—'
    running = false
    steps: DemoStep[] = []
    summary = ''

    @onMount
    async inspectStorage(): Promise<void> {
        this.storageBackend = typeof navigator.storage?.getDirectory === 'function'
            ? 'OPFS (origin-private file system)'
            : 'in-memory fallback'
        try {
            const estimate = await navigator.storage?.estimate?.()
            if (estimate?.usage !== undefined) {
                this.usageLabel = `${(estimate.usage / 1024 / 1024).toFixed(2)} MB used`
            }
        } catch {
            this.usageLabel = 'quota unavailable'
        }
    }

    // Walk a document lifecycle against a scratch collection — the same FYLO
    // semantics the server exposes, running entirely in the browser.
    async runLifecycleDemo(): Promise<void> {
        this.running = true
        this.steps = []
        this.summary = ''
        const scratch = fylo['atlas-scratch']
        const record = (op: string, detail: string) => {
            this.steps = [...this.steps, { op, detail }]
        }
        try {
            await scratch.createCollection()
            record('createCollection', 'atlas-scratch ready in the local mirror')

            const batch = await scratch.batchPut([
                { name: 'alpha', kind: 'demo' },
                { name: 'beta', kind: 'demo' },
            ]) as { ids?: unknown[] }
            record('batchPut', `stored ${(batch.ids ?? []).length} documents`)

            const single = await scratch.create({ name: 'gamma', kind: 'demo' }) as { id?: string }
            record('create', `stored ${single.id}`)

            const found = await scratch.find({}) as { docs?: unknown[] }
            record('find', `${found.docs?.length ?? 0} documents in the collection`)

            if (single.id) {
                await scratch.patch(single.id, { name: 'gamma', kind: 'patched' })
                record('patch', `updated ${single.id}`)
                await scratch.del(single.id)
                record('del', `removed ${single.id}`)
            }

            const after = await scratch.find({}) as { docs?: unknown[] }
            record('find', `${after.docs?.length ?? 0} documents remain`)

            await scratch.dropCollection()
            record('dropCollection', 'scratch collection removed')

            this.summary = `Completed ${this.steps.length} operations against the browser-local FYLO store.`
        } catch (error) {
            this.summary = error instanceof Error ? error.message : 'Lifecycle demo failed.'
        } finally {
            this.running = false
            void this.inspectStorage()
        }
    }
}
