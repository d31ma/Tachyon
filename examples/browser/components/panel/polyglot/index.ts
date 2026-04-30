type VersionedPayload = {
    message: string
    version: string
    context: { requestId: string, protocol: string, host: string, ipAddress: string }
}

export default class extends Tac {
    versioned: VersionedPayload | null = null
    loading = true

    loadingState(): string {
        return this.loading ? 'loading' : 'live'
    }

    @onMount
    async refresh(): Promise<void> {
        this.loading = true
        try {
            const response = await fetch('/languages/python/versions/v1', { cache: 'reload' })
            this.versioned = await response.json() as VersionedPayload
        } catch {
            this.versioned = null
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
