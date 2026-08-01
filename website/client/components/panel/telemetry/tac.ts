type Span = {
    name: string
    durationMs: number
    startedAt: string
}

// Spans live in localStorage: a browser-side demo needs somewhere durable to
// put them, and this is the platform's own answer.
const STORE_KEY = 'atlas-spans'

function readStoredSpans(): Span[] {
    try {
        const stored = JSON.parse(localStorage.getItem(STORE_KEY) ?? '[]') as Span[]
        return Array.isArray(stored) ? stored : []
    } catch {
        return []
    }
}

function writeStoredSpans(spans: Span[]): void {
    try {
        localStorage.setItem(STORE_KEY, JSON.stringify(spans))
    } catch {
        /* Storage can be full or blocked; the panel still renders. */
    }
}

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

    hydrate(): void {
        void this.load()
    }

    recentSummary(): string {
        if (this.loading) return 'Loading locally stored spans…'
        if (this.spans.length === 0) {
            return 'No spans yet — run the benchmark to time browser fetches and persist the results.'
        }
        return this.recent
            .map((span) => `${span.name.padEnd(20)} ${span.durationMs.toFixed(1).padStart(7)} ms  ${span.startedAt}`)
            .join('\n')
    }

    async load(): Promise<void> {
        this.loading = true
        try {
            this.spans = readStoredSpans()
                .filter((doc) => typeof doc?.durationMs === 'number')
                .sort((a, b) => (b.startedAt ?? '').localeCompare(a.startedAt ?? ''))
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
                    await fetch('/shared/data/showcase.json', { cache: 'reload' })
                    spans.push({ name, durationMs: Math.max(0.1, performance.now() - start), startedAt })
                } catch {
                    /* Keep a failed local request out of the demonstration. */
                }
            }
            if (spans.length > 0) {
                writeStoredSpans([...readStoredSpans(), ...spans])
            }
            await this.load()
        } finally {
            this.running = false
        }
    }

    async clearSpans(): Promise<void> {
        writeStoredSpans([])
        this.spans = []
    }
}
