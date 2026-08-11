// @ts-check

/**
 * @typedef {{ from: string, text: string, sentAt: string }} ChatMessage
 */

const CHANNEL = 'tachyon-atlas-realtime'
const COLLECTION = 'atlas-messages'

/** @returns {ChatMessage[]} */
function readHistory() {
  try {
    const stored = JSON.parse(localStorage.getItem(COLLECTION) ?? '[]')
    return Array.isArray(stored) ? stored : []
  } catch {
    return []
  }
}

/** @param {ChatMessage[]} messages */
function writeHistory(messages) {
  try {
    localStorage.setItem(COLLECTION, JSON.stringify(messages.slice(0, 10)))
  } catch {
    /* Storage can be full or blocked; live delivery still works. */
  }
}

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
  /** @type {HTMLElement | null} */
  root = null

  constructor() {
    try { this.$nickname = sessionStorage.getItem('atlas-nickname') ?? this.$nickname } catch {}
  }

  get isConnected() {
    return this.status === 'connected'
  }

  /** @param {HTMLElement} root @param {AbortSignal} signal */
  hydrate(root, signal) {
    this.root = root
    const nickname = root.querySelector('.rt-nickname')
    if (nickname instanceof HTMLInputElement) nickname.value = this.$nickname
    void this.connect()
    signal.addEventListener('abort', () => this.disconnect(), { once: true })
  }

  refresh() {
    void this.root?.tachyonComponent?.refresh?.()
  }

  messageSummary() {
    if (this.messages.length === 0) {
      return 'No messages yet. Open the atlas in a second tab and say hello — it arrives here without any server.'
    }
    return this.messages.map((message) => `${message.from}: ${message.text}\n${message.sentAt}`).join('\n\n')
  }

  /** @param {string} value */
  updateNickname(value) {
    this.$nickname = value
    try { sessionStorage.setItem('atlas-nickname', value) } catch {}
  }

  async connect() {
    if (this.channel) return
    // Native cross-tab delivery; local storage provides the durable history
    // a new tab replays on join.
    this.channel = new BroadcastChannel(CHANNEL)
    this.channel.onmessage = (event) => {
      this.receive(/** @type {ChatMessage} */ (event.data))
    }
    this.status = 'connected'
    await this.loadHistory()
    this.feedback = 'Open this page in a second tab and send yourself a message.'
    this.refresh()
  }

  /** @param {ChatMessage} message */
  receive(message) {
    if (!message || typeof message.text !== 'string') return
    this.messages = [message, ...this.messages].slice(0, 10)
    this.refresh()
  }

  async loadHistory() {
    this.messages = readHistory()
      .filter((doc) => typeof doc?.text === 'string')
      .sort((a, b) => (b.sentAt ?? '').localeCompare(a.sentAt ?? ''))
      .slice(0, 10)
    this.refresh()
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
    const input = this.root?.querySelector('.rt-message')
    if (input instanceof HTMLInputElement) input.value = ''
    writeHistory([message, ...readHistory()])
    this.feedback = 'Delivered live and stored — reconnecting tabs replay it from local storage.'
    this.refresh()
  }

  async clearHistory() {
    writeHistory([])
    this.messages = []
    this.feedback = 'History cleared from local storage.'
    this.refresh()
  }

  disconnect() {
    this.channel?.close()
    this.channel = null
    this.status = 'idle'
    this.feedback = 'Channel closed. Reconnect to keep chatting between tabs.'
    this.refresh()
  }
}
