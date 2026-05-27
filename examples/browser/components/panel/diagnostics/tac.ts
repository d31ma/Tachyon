type DiagnosticsPayload = {
    message: string
    runtime: { name: string, version: string }
    context: { requestId: string, protocol: string, host: string, ipAddress: string }
    frontend: string
    backend: string
}

type HealthPayload = { status: string }

type LanguageHandler = { language: string, status: string }

export default class extends Tac {
    diagnostics: DiagnosticsPayload | null = null
    health: HealthPayload | null = null
    ready: HealthPayload | null = null
    handlers: LanguageHandler[] = []
    loading = true

    loadingState(): string {
        return this.loading ? 'loading' : 'live'
    }

    get healthLabel(): string {
        if (!this.health) return '—'
        return this.health.status === 'ok' ? 'Healthy' : this.health.status
    }

    get readyLabel(): string {
        if (!this.ready) return '—'
        return this.ready.status === 'ok' ? 'Ready' : this.ready.status
    }

    get handlerCount(): number {
        return this.handlers.length
    }

    @onMount
    async refresh(): Promise<void> {
        this.loading = true
        try {
            const [diagRes, healthRes, readyRes] = await Promise.all([
                fetch('/languages/javascript', { cache: 'reload' }),
                fetch('/health', { cache: 'reload' }).catch(() => null),
                fetch('/ready', { cache: 'reload' }).catch(() => null),
            ])
            this.diagnostics = await diagRes.json() as DiagnosticsPayload
            this.health = healthRes ? await healthRes.json() as HealthPayload : null
            this.ready = readyRes ? await readyRes.json() as HealthPayload : null
        } catch {
            this.diagnostics = null
        }
        // Probe available language handlers
        const langs = [
            { path: '/languages/javascript', language: 'JavaScript' },
            { path: '/languages/typescript', language: 'TypeScript' },
            { path: '/languages/python', language: 'Python' },
            { path: '/languages/ruby', language: 'Ruby' },
            { path: '/languages/php', language: 'PHP' },
            { path: '/languages/dart', language: 'Dart' },
            { path: '/languages/java', language: 'Java' },
            { path: '/languages/csharp', language: 'C#' },
        ]
        const results: LanguageHandler[] = []
        for (const lang of langs) {
            try {
                const res = await fetch(lang.path, { cache: 'reload' })
                results.push({ language: lang.language, status: res.ok ? 'active' : 'error' })
            } catch {
                results.push({ language: lang.language, status: 'unavailable' })
            }
        }
        this.handlers = results
        this.loading = false
    }

    @onMount
    bindRefreshListener(): void {
        const handler = () => { this.refresh() }
        window.addEventListener('tachyon:refresh', handler)
    }
}
