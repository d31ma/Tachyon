type VerbRun = { verb: string, returns: string, summary: string }

export default class {
    verbRuns: VerbRun[] = [
        { verb: 'fields', returns: 'reactive state', summary: 'writes schedule a component refresh' },
        { verb: 'methods', returns: 'template action', summary: 'available without event wiring' },
        { verb: 'signals', returns: 'retained value', summary: 'publish and subscribe cross component boundaries' },
        { verb: 'native shims', returns: 'native-shaped call', summary: 'target-hosted and capability-allowlisted' },
    ]
    loading = false
    companionStatus = 'companion ABI ready'

    loadingState(): string {
        return this.loading ? 'loading' : 'live'
    }

    @subscribe('tachyon:refresh', { onMount: true })
    refresh(): void {}
}
