type UserDoc = {
    id: string
    email: string
    role: string
    apiKeyPreview: string
    createdAt: string
}

export default class extends Tac {
    users: UserDoc[] = []
    loading = true
    error: string = ''

    loadingState(): string {
        if (this.loading) return 'loading'
        if (this.error) return 'error'
        return `${this.users.length} loaded`
    }

    @onMount
    async refresh(): Promise<void> {
        this.loading = true
        this.error = ''
        try {
            // `fylo` is compiler-injected — no import needed. See
            // src/runtime/fylo-global.js + Compiler.referencesFyloGlobal.
            const result = await fylo.users.find()
            if (result.error) {
                this.error = result.error
                this.users = []
                return
            }
            this.users = (result.docs ?? []).map((entry: { id: string; doc: unknown }) => entry.doc as UserDoc)
        } catch (e) {
            this.error = e instanceof Error ? e.message : String(e)
            this.users = []
        } finally {
            this.loading = false
        }
    }

    @onMount
    bindRefreshListener(): void {
        const handler = () => { this.refresh() }
        window.addEventListener('tachyon:refresh', handler)
    }
}
