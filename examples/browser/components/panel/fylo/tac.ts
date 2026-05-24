type FyloCollection = {
    name: string
    exists: boolean
    docsStored?: number
    indexedDocs?: number
    worm?: boolean
    error?: string
}

type FyloStatus = {
    enabled: boolean
    root?: string
    collections?: FyloCollection[]
}

export default class extends Tac {
    // Local state — named `info` (not `fylo`) to avoid shadowing the
    // compiler-injected global `fylo` in the rendered template scope.
    info: FyloStatus | null = null
    loading = true

    loadingState(): string {
        return this.loading ? 'loading' : 'live'
    }

    docsTotal(): number {
        if (!this.info?.collections) return 0
        return this.info.collections.reduce((sum, entry) => sum + (entry.docsStored ?? 0), 0)
    }

    @onMount
    async refresh(): Promise<void> {
        this.loading = true
        try {
            // Compiler-injected `fylo` global — no manual import or fetch wiring.
            // Returns `{ root: '', collections: [] }` if /_fylo isn't mounted.
            const payload = await fylo.collections() as { root: string, collections: FyloCollection[] }
            const enabled = Boolean(payload.root)
            this.info = enabled
                ? { enabled: true, root: payload.root, collections: payload.collections }
                : { enabled: false }
        } catch {
            this.info = { enabled: false }
        } finally {
            this.loading = false
        }
    }

    @onMount
    bindRefreshListener(): void {
        const handler = () => { this.refresh() }
        window.addEventListener('tachyon:refresh', handler)
    }
}
