export default class {
    sourceNote = 'storage ready'
    loading = false

    loadingState(): string {
        return this.loading ? 'loading' : 'live'
    }

    @subscribe('tachyon:refresh', { onMount: true })
    async refresh(): Promise<void> {
        localStorage.setItem('tachyon.language.typescript', 'ready')
        const result = await fylo.collection('tac-companion-probes').find({ limit: 1 })
        const storage = localStorage.getItem('tachyon.language.typescript') ?? 'storage unavailable'
        this.sourceNote = `${storage} - FYLO ${result.local ? 'local' : 'ready'}`
    }

    @publish('tachyon:wave')
    wave(): string {
        return 'TypeScript'
    }
}
