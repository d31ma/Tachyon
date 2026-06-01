// @ts-check

export default class extends Tac {
    /** @type {string} */
    $nickname = 'Ada'
    /** @type {string} */
    $$clientId = ''
    /** @type {string} */
    targetClientId = ''
    /** @type {string} */
    messageText = 'Hello from a durable Yon mailbox'
    /** @type {Array<{ event: string, text: string, from?: string, sentAt?: string }>} */
    messages = []
    /** @type {Array<{ clientId: string, nickname: string, streamUrl: string, registeredAt: string }>} */
    clients = []
    /** @type {string} */
    status = 'idle'
    /** @type {string} */
    feedback = ''
    /** @type {number} */
    cursor = 0
    /** @type {AbortController | null} */
    streamController = null

    get isConnected() {
        return this.status === 'connected'
    }

    get clientLabel() {
        return this.$$clientId ? `${this.$nickname} (${this.$$clientId})` : 'not registered'
    }

    async register() {
        const nickname = this.$nickname.trim() || 'Browser guest'
        const response = await fetch('/realtime/clients', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ nickname }),
        })
        const payload = await response.json()
        if (!response.ok || payload.detail) {
            this.feedback = payload.detail ?? 'Unable to register realtime client.'
            return
        }
        this.$$clientId = String(payload.clientId)
        this.feedback = `Registered ${nickname}. Open another tab and copy this client id to send a message.`
        await this.refreshClients()
    }

    async refreshClients() {
        const response = await fetch('/realtime/clients', { cache: 'reload' })
        if (!response.ok) return
        const payload = await response.json()
        this.clients = Array.isArray(payload.clients) ? payload.clients : []
    }

    async connect() {
        if (!this.$$clientId) await this.register()
        if (!this.$$clientId || this.streamController) return
        const controller = new AbortController()
        this.streamController = controller
        this.status = 'connected'
        this.feedback = 'Listening for Yon realtime messages.'
        try {
            const url = `/_yon/realtime/stream?clientId=${encodeURIComponent(this.$$clientId)}&cursor=${this.cursor}`
            const response = await fetch(url, {
                cache: 'no-store',
                headers: { accept: 'text/event-stream' },
                signal: controller.signal,
            })
            if (!response.ok || !response.body) throw new Error(`Realtime stream failed with ${response.status}`)
            await this.readEventStream(response.body)
        }
        catch (error) {
            if (!controller.signal.aborted) {
                this.status = 'error'
                this.feedback = error instanceof Error ? error.message : 'Realtime stream failed.'
            }
        }
        finally {
            if (this.streamController === controller) this.streamController = null
        }
    }

    disconnect() {
        this.streamController?.abort()
        this.streamController = null
        this.status = 'idle'
        this.feedback = 'Realtime stream disconnected. Messages will replay from the durable cursor when you reconnect.'
    }

    async send() {
        const text = this.messageText.trim()
        if (!this.$$clientId) await this.register()
        if (!this.targetClientId || !text || !this.$$clientId) {
            this.feedback = 'Choose a target client and write a message first.'
            return
        }
        const response = await fetch('/realtime/messages', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ from: this.$$clientId, to: this.targetClientId, text }),
        })
        const payload = await response.json()
        if (!response.ok || payload.detail) {
            this.feedback = payload.detail ?? 'Unable to send realtime message.'
            return
        }
        this.feedback = `Stored message ${payload.id}. If the target is online, their SSE stream will receive it.`
        this.messageText = ''
    }

    /**
     * @param {ReadableStream<Uint8Array>} body
     */
    async readEventStream(body) {
        const decoder = new TextDecoder()
        let buffer = ''
        for await (const chunk of body) {
            buffer += decoder.decode(chunk, { stream: true })
            const events = buffer.split('\n\n')
            buffer = events.pop() ?? ''
            for (const eventText of events) this.applyEvent(eventText)
        }
    }

    /** @param {string} eventText */
    applyEvent(eventText) {
        const lines = eventText.split('\n')
        const id = lines.find(line => line.startsWith('id:'))?.slice(3).trim()
        const event = lines.find(line => line.startsWith('event:'))?.slice(6).trim() || 'message'
        const data = lines
            .filter(line => line.startsWith('data:'))
            .map(line => line.slice(5).trimStart())
            .join('\n')
        if (id) this.cursor = Number(id) || this.cursor
        if (!data || event === 'ready') return
        try {
            const payload = JSON.parse(data)
            const message = payload.data ?? payload
            this.messages = [{
                event,
                text: String(message.text ?? data),
                from: typeof message.from === 'string' ? message.from : undefined,
                sentAt: typeof message.sentAt === 'string' ? message.sentAt : undefined,
            }, ...this.messages].slice(0, 10)
        }
        catch {
            this.messages = [{ event, text: data }, ...this.messages].slice(0, 10)
        }
    }

    @onMount
    async prepare() {
        await this.refreshClients()
        if (this.$$clientId) this.connect()
    }
}
