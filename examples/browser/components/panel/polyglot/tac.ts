type VersionedPayload = {
    message: string
    version: string
    context: { requestId: string, protocol: string, host: string, ipAddress: string }
}

type WorkerPayload = {
    method: string
    result: unknown
}

type WorkerRun = WorkerPayload & {
    language: string
}

export default class extends Tac {
    versioned: VersionedPayload | null = null
    workerResults: WorkerRun[] = []
    loading = true

    loadingState(): string {
        return this.loading ? 'loading' : 'live'
    }

    formatResult(result: unknown): string {
        return typeof result === 'object' && result !== null
            ? JSON.stringify(result)
            : String(result)
    }

    @subscribe('tachyon:refresh', { onMount: true })
    async refresh(): Promise<void> {
        this.loading = true
        try {
            const response = await fetch('/languages/python/versions/v1', { cache: 'reload' })
            this.versioned = await response.json() as VersionedPayload
            const body = this.versioned.message
            const languages = [
                { language: 'Rust', url: 'tac://language/rust?pool=2' },
                { language: 'C', url: 'tac://language/c' },
                { language: 'C++', url: 'tac://language/cpp' },
                { language: 'Zig', url: 'tac://language/zig' },
                { language: 'Python', url: 'tac://language/python' },
                { language: 'C#', url: 'tac://language/csharp' },
                { language: 'Go', url: 'tac://language/go' },
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
            this.versioned = null
            this.workerResults = []
        } finally {
            this.loading = false
        }
    }
}
