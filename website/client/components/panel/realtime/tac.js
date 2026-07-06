// @ts-check

/**
 * @typedef {{ from: string, text: string, sentAt: string }} ChatMessage
 * @typedef {{ id: string, doc: ChatMessage }} ChatDocEntry
 */

const CHANNEL = 'tachyon-atlas-realtime'
const COLLECTION = 'atlas-messages'

export default class {
  /** @type {string} */
  $nickname = 'Ada'
  /** @type {string} */
  messageText = 'Hello from another tab'
  /** @type {ChatMessage[]} */
  messages = []
  /** @type {string} */
  status = 'idle'
  /** @type {string} */
  feedback = ''
  /** @type {BroadcastChannel | null} */
  channel = null

  get isConnected() {
    return this.status === 'connected'
  }

  @onMount
  async connect() {
    if (this.channel) return
    // Native cross-tab delivery; FYLO's OPFS mirror provides the durable
    // history a new tab replays on join.
    this.channel = new BroadcastChannel(CHANNEL)
    this.channel.onmessage = (event) => {
      this.receive(/** @type {ChatMessage} */ (event.data))
    }
    this.status = 'connected'
    await this.loadHistory()
    this.feedback = 'Open this page in a second tab and send yourself a message.'
  }

  /** @param {ChatMessage} message */
  receive(message) {
    if (!message || typeof message.text !== 'string') return
    this.messages = [message, ...this.messages].slice(0, 10)
  }

  async loadHistory() {
    try {
      const result = /** @type {{ docs?: ChatDocEntry[] }} */ (await fylo[COLLECTION].find({}))
      this.messages = (result.docs ?? [])
        .map((entry) => entry.doc)
        .filter((doc) => typeof doc?.text === 'string')
        .sort((a, b) => (b.sentAt ?? '').localeCompare(a.sentAt ?? ''))
        .slice(0, 10)
    } catch {
      this.messages = []
    }
  }

  async send() {
    const text = this.messageText.trim()
    if (!text) {
      this.feedback = 'Write a message first.'
      return
    }
    /** @type {ChatMessage} */
    const message = {
      from: this.$nickname.trim() || 'Browser guest',
      text,
      sentAt: new Date().toISOString(),
    }
    this.channel?.postMessage(message)
    this.receive(message)
    this.messageText = ''
    const stored = /** @type {{ ok?: boolean, id?: string }} */ (await fylo[COLLECTION].create(message))
    this.feedback = stored.ok
      ? `Delivered live and stored as ${stored.id} — reconnecting tabs replay it from OPFS.`
      : 'Delivered live; storing the history failed.'
  }

  async clearHistory() {
    try {
      const result = /** @type {{ docs?: ChatDocEntry[] }} */ (await fylo[COLLECTION].find({}))
      for (const entry of result.docs ?? []) {
        await fylo[COLLECTION].del(entry.id)
      }
    } catch {
      /* nothing stored yet */
    }
    this.messages = []
    this.feedback = 'History cleared from the local mirror.'
  }

  disconnect() {
    this.channel?.close()
    this.channel = null
    this.status = 'idle'
    this.feedback = 'Channel closed. Reconnect to keep chatting between tabs.'
  }
}
