type WorkerEnvelope = {
    method: string
    result: unknown
}

type VerbRun = { verb: string, returns: string, summary: string }

export default class {
    verbRuns: VerbRun[] = []
    loading = true
    wasmStatus = 'compiling…'

    loadingState(): string {
        return this.loading ? 'loading' : 'live'
    }

    @subscribe('tachyon:refresh', { onMount: true })
    async refresh(): Promise<void> {
        this.loading = true
        // One browser-local backend, every verb: the Rust worker compiled
        // in-house to tac.wasm dispatches the fetch method to the matching
        // handler method — no server round-trip anywhere in this panel.
        const probeBody = 'diagnostics probe from the atlas'
        const calls: Array<{ verb: string, returns: string, init?: RequestInit }> = [
            { verb: 'GET', returns: 'i32' },
            { verb: 'POST', returns: 'i32', init: { method: 'POST', body: probeBody } },
            { verb: 'PUT', returns: 'i32', init: { method: 'PUT', body: probeBody } },
            { verb: 'PATCH', returns: 'Json', init: { method: 'PATCH', body: JSON.stringify({ language: 'Rust', message: probeBody }) } },
            { verb: 'DELETE', returns: 'bool', init: { method: 'DELETE', body: probeBody } },
        ]
        this.verbRuns = await Promise.all(calls.map(async (call): Promise<VerbRun> => {
            try {
                const response = await fetch('tac://language/rust', call.init ?? {})
                const payload = await response.json() as WorkerEnvelope
                return { verb: call.verb, returns: call.returns, summary: this.format(payload.result) }
            } catch {
                return { verb: call.verb, returns: call.returns, summary: 'unavailable' }
            }
        }))
        this.wasmStatus = this.verbRuns.some((run) => run.summary !== 'unavailable')
            ? 'tac.wasm live'
            : 'worker unavailable'
        this.loading = false
    }

    private format(result: unknown): string {
        const text = typeof result === 'object' && result !== null ? JSON.stringify(result) : String(result)
        return text.length > 90 ? `${text.slice(0, 90)}…` : text
    }
}
