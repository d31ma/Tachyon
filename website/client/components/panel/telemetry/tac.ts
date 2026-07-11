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

    // Time normal local-first companion fetches and persist the spans.
    async runBenchmark(): Promise<void> {
        this.running = true
        try {
            const calls = ['showcase data', 'docs data', 'web environment']
            const spans: Span[] = []
            for (const name of calls) {
                const startedAt = new Date().toISOString()
                const start = performance.now()
                try {
                    await this.tac.fetch('/shared/data/showcase.json', { cache: 'reload' })
                    spans.push({ name, durationMs: Math.max(0.1, performance.now() - start), startedAt })
                } catch {
                    /* Keep a failed local request out of the demonstration. */
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
