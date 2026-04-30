type TelemetrySpan = {
    traceId: string
    spanId: string
    parentSpanId: string
    kind: string
    name: string
    requestId: string | null
    route: string | null
    method: string | null
    statusCode: number | null
    traceState: string
    startTimeUnixNano: string
    endTimeUnixNano: string
    durationMs: number
}

type TelemetryPayload = {
    summary: {
        enabled: boolean
        collection: string
        spanCount: number
        requestCount: number
        errorCount: number
    }
    recent: TelemetrySpan[]
}

export default class extends Tac {
    telemetry: TelemetryPayload | null = null
    loading = true

    loadingState(): string {
        return this.loading ? 'loading' : 'live'
    }

    @onMount
    async refresh(): Promise<void> {
        this.loading = true
        try {
            const response = await fetch('/languages/javascript/telemetry?limit=6', { cache: 'reload' })
            this.telemetry = await response.json() as TelemetryPayload
        } catch {
            this.telemetry = null
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
