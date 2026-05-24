type DiagnosticsPayload = {
    message: string
    runtime: { name: string, version: string }
    context: { requestId: string, protocol: string, host: string, ipAddress: string }
    frontend: string
    backend: string
}

export default class extends Tac {
    diagnostics: DiagnosticsPayload | null = null
    loading = true

    loadingState(): string {
        return this.loading ? 'loading' : 'live'
    }

    @onMount
    async refresh(): Promise<void> {
        this.loading = true
        try {
            const response = await fetch('/languages/javascript', { cache: 'reload' })
            this.diagnostics = await response.json() as DiagnosticsPayload
        } catch {
            this.diagnostics = null
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
