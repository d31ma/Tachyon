type UserDoc = {
    id: string
    email: string
    role: string
    apiKeyPreview: string
}

type CachePolicy = 'cache-first' | 'network-first' | 'reload' | 'no-store'

type FyloDocEntry = {
    id: string
    doc: unknown
}

type FyloFindResult = {
    docs?: FyloDocEntry[]
    error?: string
}

type FyloMutationResult = {
    ok?: boolean
    id?: string
    error?: string
}

type FyloSubscribeMeta = {
    collection: string
    events: unknown[]
    offset: number
    source: 'initial' | 'event-stream' | 'poll' | 'local'
}

type CacheRun = {
    policy: CachePolicy
    count: number
    duration: string
    note: string
}

export default class {
    users: UserDoc[] = []
    cacheRuns: CacheRun[] = []
    cachePolicies: CachePolicy[] = ['cache-first', 'network-first', 'reload', 'no-store']
    selectedPolicy: CachePolicy = 'network-first'
    loading = true
    mutating = false
    error: string = ''
    mutationLog: string = 'Mutation demo is idle. It creates and removes a temporary user in the browser-local mirror.'
    syncStatus = 'Opening FYLO subscription...'
    private unsubscribeUsers: (() => void) | null = null

    loadingState(): string {
        if (this.loading) return `loading ${this.selectedPolicy}`
        if (this.mutating) return 'mutating'
        if (this.error) return 'error'
        return `${this.users.length} loaded`
    }

    @subscribe('tachyon:refresh')
    async refresh(): Promise<void> {
        await this.loadWithPolicy('network-first')
    }

    @onMount
    syncUsers(): void {
        if (this.unsubscribeUsers) return
        const startedAt = performance.now()
        this.unsubscribeUsers = fylo.users.subscribe(
            { order: 'email.asc' },
            (result: FyloFindResult, meta: FyloSubscribeMeta) => {
                const duration = `${Math.max(1, Math.round(performance.now() - startedAt))}ms`
                const eventLabel = meta.events.length === 1 ? 'event' : 'events'
                const note = meta.source === 'initial'
                    ? 'subscribe() loaded users and opened the FYLO event stream'
                    : `subscribe() refreshed after ${meta.events.length} FYLO ${eventLabel}`
                this.loading = false
                this.syncStatus = meta.source === 'initial'
                    ? `listening at offset ${meta.offset}`
                    : `synced ${meta.events.length} ${eventLabel} from ${meta.source}`
                this.applyUsersResult(result, meta.source === 'initial' ? 'network-first' : 'reload', duration, note)
            },
            {
                cache: 'network-first',
                pollMs: 1000,
                onError: (error: unknown) => {
                    this.error = error instanceof Error ? error.message : String(error)
                    this.syncStatus = 'subscription paused; see error'
                },
            },
        )
    }

    async loadWithPolicy(policy: CachePolicy): Promise<void> {
        this.selectedPolicy = policy
        this.loading = true
        this.error = ''
        try {
            await this.loadUsers(policy, this.policyNote(policy))
        } finally {
            this.loading = false
        }
    }

    async runMutationDemo(): Promise<void> {
        this.mutating = true
        this.error = ''
        const email = `cache-${Date.now()}@example.test`
        this.mutationLog = `Creating ${email} through fylo.users.create()...`
        try {
            const created = await fylo.users.create({
                email,
                role: 'viewer',
                apiKeyPreview: 'ak_test_cacheDemo1234',
            }) as FyloMutationResult

            if (created.error) {
                this.mutationLog = created.error
                return
            }

            await this.loadUsers('cache-first', 'create() invalidated cached users, so cache-first refreshes')

            if (created.id) {
                const deleted = await fylo.users.del(created.id) as FyloMutationResult
                if (deleted.error) {
                    this.mutationLog = `Created ${email}, but cleanup failed: ${deleted.error}`
                    return
                }
            }

            await this.loadUsers('network-first', 'cleanup delete() invalidated the collection again')
            this.mutationLog = `Created and removed ${email}. Both mutations invalidated cached users reads.`
        } catch (e) {
            this.error = e instanceof Error ? e.message : String(e)
            this.mutationLog = this.error
        } finally {
            this.mutating = false
        }
    }

    private async loadUsers(policy: CachePolicy, note: string): Promise<void> {
        const startedAt = performance.now()
        // `fylo` is compiler-injected — no import needed. See
        // src/runtime/fylo-global.js + Compiler.referencesFyloGlobal.
        const result = await fylo.users.find({ order: 'email.asc' }, { cache: policy }) as FyloFindResult
        const duration = `${Math.max(1, Math.round(performance.now() - startedAt))}ms`
        this.applyUsersResult(result, policy, duration, note)
    }

    private applyUsersResult(result: FyloFindResult, policy: CachePolicy, duration: string, note: string): void {
        if (result.error) {
            this.error = result.error
            this.users = []
            this.recordCacheRun(policy, 0, duration, result.error)
            return
        }

        this.users = (result.docs ?? []).map((entry) => ({
            id: entry.id,
            ...(entry.doc as Omit<UserDoc, 'id'>),
        }))
        this.recordCacheRun(policy, this.users.length, duration, note)
    }

    private recordCacheRun(policy: CachePolicy, count: number, duration: string, note: string): void {
        this.cacheRuns = [
            { policy, count, duration, note },
            ...this.cacheRuns,
        ].slice(0, 5)
    }

    maskedApiKey(value: string): string {
        if (!value) return 'no key'
        const parts = value.split('_')
        const suffix = value.slice(-4)
        return parts.length >= 3
            ? `${parts[0]}_${parts[1]}_****${suffix}`
            : `****${suffix}`
    }

    private policyNote(policy: CachePolicy): string {
        switch (policy) {
            case 'cache-first':
                return 'reads the local FYLO mirror first, then network only on a miss'
            case 'network-first':
                return 'tries network first and falls back to the local mirror offline'
            case 'reload':
                return 'bypasses local reads and refreshes the mirror from network'
            case 'no-store':
                return 'skips both reading and writing the local mirror'
            default:
                return 'unknown cache policy'
        }
    }
}
