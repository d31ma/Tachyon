type ShowcasePayload = {
    title?: string
    summary?: string
}

type WorkerPayload = {
    method: string
    result: unknown
}

type WorkerRun = WorkerPayload & {
    language: string
}

export default class {
    workerResults: WorkerRun[] = []
    sourceNote = ''
    loading = true

    loadingState(): string {
        return this.loading ? 'loading' : 'live'
    }

    formatResult(result: unknown): string {
        const text = typeof result === 'object' && result !== null
            ? JSON.stringify(result)
            : String(result)
        return text.length > 80 ? `${text.slice(0, 80)}…` : text
    }

    @subscribe('tachyon:refresh', { onMount: true })
    async refresh(): Promise<void> {
        this.loading = true
        try {
            // Local-first shared data: this read hydrates from IndexedDB on
            // repeat visits before touching the network.
            const response = await fetch('/shared/data/showcase.json')
            const showcase = await response.json() as ShowcasePayload
            const body = showcase.summary ?? 'Tachyon workers speak every verb.'
            this.sourceNote = showcase.title ?? 'Tac local-first data'
            // Three languages, one worker convention — each compiled by
            // Tachyon itself to tac.wasm. Rust opts into a pooled dispatch.
            const languages = [
                { language: 'Rust', url: 'tac://language/rust?pool=2' },
                { language: 'JavaScript', url: 'tac://language/javascript' },
                { language: 'TypeScript', url: 'tac://language/typescript' },
            ]
            const workers = languages.flatMap((worker) => [
                { ...worker, method: 'POST', body },
                { ...worker, method: 'PATCH', body: { language: worker.language, message: body } },
                { ...worker, method: 'DELETE', body },
            ])
            const results = await Promise.all(workers.map(async (worker) => {
                const workerResponse = await fetch(worker.url, {
                    method: worker.method,
                    body: worker.body as BodyInit,
                })
                const payload = await workerResponse.json() as WorkerPayload
                return { language: worker.language, ...payload }
            }))
            this.workerResults = results
        } catch {
            this.workerResults = []
        } finally {
            this.loading = false
        }
    }
}
