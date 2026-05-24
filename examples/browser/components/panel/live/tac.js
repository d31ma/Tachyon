// @ts-check

/**
 * @typedef {object} WikimediaRecentChange
 * @property {number|string} [timestamp]
 */

const WIKIMEDIA_RECENTCHANGE_URL = 'https://stream.wikimedia.org/v2/stream/recentchange'
const ONE_HOUR_MS = 60 * 60 * 1000

export default class extends Tac {
    /** @type {number[]} */
    editTimestamps = []
    status = 'idle'
    lastError = ''
    listenRequested = false
    /** @type {AbortController | null} */
    streamController = null
    get isStreaming() {
        return this.status === 'streaming'
    }

    get editsLastHour() {
        this.pruneEditTimestamps()
        return this.editTimestamps.length
    }

    statusLabel() {
        if (this.status === 'streaming') return 'streaming'
        if (this.status === 'complete') return 'complete'
        if (this.status === 'error') return 'offline'
        return 'ready'
    }

    stop() {
        this.listenRequested = false
        this.streamController?.abort()
        this.streamController = null
        if (this.status === 'streaming') this.status = 'idle'
    }

    async start() {
        if (this.listenRequested)
            return
        this.stop()
        this.listenRequested = true
        this.editTimestamps = []
        this.status = 'streaming'
        this.lastError = ''
        const controller = new AbortController()
        this.streamController = controller

        try {
            const response = await fetch(WIKIMEDIA_RECENTCHANGE_URL, {
                cache: 'no-store',
                headers: { accept: 'text/event-stream' },
                signal: controller.signal,
            })
            if (!response.ok || !response.body)
                throw new Error(`Stream failed with ${response.status}`)

            await this.readEventStream(response.body)
            if (!controller.signal.aborted) {
                this.status = 'complete'
                this.listenRequested = false
            }
        }
        catch (error) {
            if (controller.signal.aborted)
                return
            this.status = 'error'
            this.lastError = error instanceof Error ? error.message : 'Unable to read live stream.'
            this.listenRequested = false
        }
        finally {
            if (this.streamController === controller)
                this.streamController = null
        }
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
            for (const eventText of events)
                this.applyEvent(eventText)
        }
    }

    /**
     * @param {string} eventText
     */
    applyEvent(eventText) {
        const data = eventText
            .split('\n')
            .filter(line => line.startsWith('data:'))
            .map(line => line.slice(5).trimStart())
            .join('\n')
        if (!data)
            return
        try {
            this.recordEdit(/** @type {WikimediaRecentChange} */ (JSON.parse(data)))
        }
        catch {
            this.lastError = 'Skipped a malformed stream event.'
        }
    }

    /**
     * @param {WikimediaRecentChange} event
     */
    recordEdit(event) {
        const timestamp = this.toTimestampMs(event.timestamp)
        if (!timestamp)
            return

        this.editTimestamps = [...this.editTimestamps, timestamp]
        this.pruneEditTimestamps(timestamp)
    }

    /**
     * @param {number|string|undefined} value
     * @returns {number | null}
     */
    toTimestampMs(value) {
        if (typeof value === 'number')
            return value * 1000
        if (typeof value === 'string') {
            const parsed = Date.parse(value)
            return Number.isNaN(parsed) ? null : parsed
        }
        return Date.now()
    }

    /**
     * @param {number} [now]
     */
    pruneEditTimestamps(now = Date.now()) {
        const cutoff = now - ONE_HOUR_MS
        const next = this.editTimestamps.filter(timestamp => timestamp >= cutoff)
        if (next.length !== this.editTimestamps.length)
            this.editTimestamps = next
    }

    @onMount
    prepare() {
        this.status = 'idle'
    }
}
