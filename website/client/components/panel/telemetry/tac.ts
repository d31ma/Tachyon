type Span = {
    name: string
    durationMs: number
    startedAt: string
}

type SpanDocEntry = { id: string, doc: Span }

export default class {
    spans: Span[] = []
    running = false
    loading = true

    loadingState(): string {
        if (this.running) return 'benchmarking'
        return this.loading ? 'loading' : 'live'
    }

    get spanCount(): number {
        return this.spans.length
    }

    get averageMs(): string {
        if (this.spans.length === 0) return '—'
        const total = this.spans.reduce((sum, span) => sum + span.durationMs, 0)
        return `${(total / this.spans.length).toFixed(1)} ms`
    }

    get slowestMs(): string {
        if (this.spans.length === 0) return '—'
        return `${Math.max(...this.spans.map((span) => span.durationMs)).toFixed(1)} ms`
    }

    get recent(): Span[] {
        return this.spans.slice(0, 8)
    }

    @subscribe('tachyon:refresh', { onMount: true })
    async load(): Promise<void> {
        this.loading = true
        try {
            const result = await fylo['atlas-spans'].find({}) as { docs?: SpanDocEntry[] }
            this.spans = (result.docs ?? [])
                .map((entry) => entry.doc)
                .filter((doc) => typeof doc?.durationMs === 'number')
                .sort((a, b) => (b.startedAt ?? '').localeCompare(a.startedAt ?? ''))
        } catch {
            this.spans = []
        } finally {
            this.loading = false
        }
    }

    // Time a burst of worker calls and persist the spans — telemetry
    // recorded, stored and queried entirely client-side.
    async runBenchmark(): Promise<void> {
        this.running = true
        try {
            const calls = [
                { name: 'rust GET', url: 'tac://language/rust', init: {} as RequestInit },
                { name: 'rust POST', url: 'tac://language/rust?pool=2', init: { method: 'POST', body: 'benchmark payload' } },
                { name: 'rust PUT', url: 'tac://language/rust', init: { method: 'PUT', body: 'benchmark payload' } },
                { name: 'javascript GET', url: 'tac://language/javascript', init: {} },
                { name: 'javascript POST', url: 'tac://language/javascript', init: { method: 'POST', body: 'benchmark payload' } },
                { name: 'typescript GET', url: 'tac://language/typescript', init: {} },
                { name: 'typescript POST', url: 'tac://language/typescript', init: { method: 'POST', body: 'benchmark payload' } },
            ]
            const spans: Span[] = []
            for (const call of calls) {
                const startedAt = new Date().toISOString()
                const start = performance.now()
                try {
                    await fetch(call.url, call.init)
                    spans.push({ name: call.name, durationMs: Math.max(0.1, performance.now() - start), startedAt })
                } catch {
                    /* skip spans for unavailable workers */
                }
            }
            if (spans.length > 0) {
                await fylo['atlas-spans'].batchPut(spans)
            }
            await this.load()
        } finally {
            this.running = false
        }
    }

    async clearSpans(): Promise<void> {
        try {
            const result = await fylo['atlas-spans'].find({}) as { docs?: SpanDocEntry[] }
            for (const entry of result.docs ?? []) {
                await fylo['atlas-spans'].del(entry.id)
            }
        } catch {
            /* nothing stored */
        }
        this.spans = []
    }
}
