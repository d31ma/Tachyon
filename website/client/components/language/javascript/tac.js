export default class {
    status = 'checking'
    lastWave = 'none yet'
    $$waves = 0

    @subscribe('tachyon:refresh', { onMount: true })
    async refresh() {
        localStorage.setItem('tachyon.language.javascript', 'ready')
        const result = await fylo.collection('tac-companion-probes').find({ limit: 1 })
        const storage = localStorage.getItem('tachyon.language.javascript') ?? 'storage unavailable'
        this.status = `${storage} - FYLO ${result.local ? 'local' : 'ready'}`
    }

    @subscribe('tachyon:wave')
    onWave(language) {
        this.lastWave = String(language)
        this.$$waves += 1
    }

    @publish('tachyon:wave')
    wave() {
        return 'JavaScript'
    }
}
