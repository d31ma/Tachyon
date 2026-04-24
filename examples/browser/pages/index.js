// @ts-check
import dayjs from 'dayjs'

/**
 * @typedef {{
 *   message: string
 *   runtime: { name: string, version: string }
 *   context: { requestId: string, protocol: string, host: string, ipAddress: string }
 *   frontend: string
 *   backend: string
 * }} DiagnosticsPayload
 *
 * @typedef {{
 *   message: string
 *   version: string
 *   context: { requestId: string, protocol: string, host: string, ipAddress: string }
 * }} VersionedPayload
 *
 * @typedef {{ id: string, name: string, source: string, createdAt: string }} InventoryItem
 * @typedef {{ title: string, summary: string }} ShowcasePayload
 * @typedef {{
 *   summary: { enabled: boolean, collection: string, spanCount: number, requestCount: number, errorCount: number },
 *   recent: Array<{
 *     traceId: string,
 *     spanId: string,
 *     parentSpanId: string,
 *     kind: string,
 *     name: string,
 *     requestId: string | null,
 *     route: string | null,
 *     method: string | null,
 *     statusCode: number | null,
 *     traceState: string,
 *     startTimeUnixNano: string,
 *     endTimeUnixNano: string,
 *     durationMs: number,
 *   }>
 * }} TelemetryPayload
 * @typedef {'diagnostics' | 'versioned' | 'items' | 'health' | 'showcase' | 'telemetry'} LoadingKey
 * @typedef {import('../../../src/runtime/tac.js').TacRuntimeBindings} TacRuntimeBindings
 */

export default class extends Tac {
  /** @type {boolean} */
  $sidebarOpen = true
  /** @type {string} */
  $theme = 'light'
  /** @type {number} */
  $visits = 0
  /** @type {string} */
  $draftNote = 'TACHYON v2 ships pure JavaScript + strict JSDoc across Tac and Yon.'
  /** @type {string} */
  release = 'TACHYON 2.0.0'
  /** @type {string} */
  headline = 'One app, two layers, every route talking live'
  /** @type {string} */
  subtitle = 'Tac drives the browser experience while Yon executes file-routed backend handlers in multiple languages.'
  /** @type {DiagnosticsPayload | null} */
  diagnostics = null
  /** @type {VersionedPayload | null} */
  versioned = null
  /** @type {InventoryItem[]} */
  items = []
  /** @type {ShowcasePayload | null} */
  showcase = null
  /** @type {TelemetryPayload | null} */
  telemetry = null
  /** @type {{ health: string, ready: string, summary: string }} */
  health = { health: 'loading', ready: 'loading', summary: 'loading' }
  /** @type {string} */
  newItem = ''
  /** @type {string} */
  feedback = ''
  /** @type {Intl.DateTimeFormat} */
  dateFormatter = new Intl.DateTimeFormat('en-CA', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  })
  /** @type {{ diagnostics: boolean, versioned: boolean, items: boolean, health: boolean, showcase: boolean, telemetry: boolean }} */
  loading = {
    diagnostics: true,
    versioned: true,
    items: true,
    health: true,
    showcase: true,
    telemetry: true,
  }

  /**
   * @param {Record<string, unknown>} [props]
   * @param {TacRuntimeBindings} [tac]
   */
  constructor(props = {}, tac = undefined) {
    super(props, tac)
    this.provide('demo-release', this.release)
    this.$visits += 1

    if (this.isBrowser) {
      document.title = 'TACHYON 2.0.0 Showcase'
      this.applyTheme()
      this.onMount(() => this.refreshAll())
    }
  }

  /** @param {LoadingKey} key */
  loadingState(key) {
    return this.loading[key] ? 'loading' : 'live'
  }

  /** @returns {void} */
  toggleSidebar() {
    this.$sidebarOpen = !this.$sidebarOpen
  }

  /** @returns {void} */
  toggleTheme() {
    this.$theme = this.$theme === 'dark' ? 'light' : 'dark'
    this.applyTheme()
  }

  /** @returns {void} */
  applyTheme() {
    document.documentElement.setAttribute('data-theme', this.$theme)
    document.documentElement.style.colorScheme = this.$theme
  }

  /** @returns {void} */
  clearDraft() {
    this.$draftNote = ''
  }

  /** @param {string | null | undefined} value */
  formatDate(value) {
    if (!value) return 'just now'
    return dayjs(value).isValid()
      ? this.dateFormatter.format(dayjs(value).toDate())
      : 'just now'
  }

  /** @returns {Promise<void>} */
  async refreshAll() {
    this.feedback = ''
    try {
      await Promise.all([
        this.loadDiagnostics(),
        this.loadVersionedRoute(),
        this.refreshItems(),
        this.loadHealth(),
        this.loadShowcaseData(),
        this.loadTelemetrySummary(),
      ])
    } finally {
      this.rerender()
    }
  }

  /** @returns {Promise<void>} */
  async loadDiagnostics() {
    this.loading.diagnostics = true
    const response = await this.fetch('/api', { cache: 'reload' })
    this.diagnostics = /** @type {DiagnosticsPayload} */ (await response.json())
    this.loading.diagnostics = false
  }

  /** @returns {Promise<void>} */
  async loadVersionedRoute() {
    this.loading.versioned = true
    const response = await this.fetch('/api/v2', { cache: 'reload' })
    this.versioned = /** @type {VersionedPayload} */ (await response.json())
    this.loading.versioned = false
  }

  /** @returns {Promise<void>} */
  async loadHealth() {
    this.loading.health = true
    const [healthRes, readyRes] = await Promise.all([
      this.fetch('/health', { cache: 'reload' }),
      this.fetch('/ready', { cache: 'reload' }),
    ])
    const [healthData, readyData] = /** @type {[{ status: string }, { status: string }]} */ (await Promise.all([
      healthRes.json(),
      readyRes.json(),
    ]))
    this.health = {
      health: healthData.status,
      ready: readyData.status,
      summary: `${healthData.status}/${readyData.status}`,
    }
    this.loading.health = false
  }

  /** @returns {Promise<void>} */
  async refreshItems() {
    this.loading.items = true
    const response = await this.fetch('/items', { cache: 'reload' })
    const payload = /** @type {{ items?: InventoryItem[] }} */ (await response.json())
    this.items = payload.items ?? []
    this.loading.items = false
  }

  /** @returns {Promise<void>} */
  async loadShowcaseData() {
    this.loading.showcase = true
    const response = await this.fetch('/shared/data/showcase.json', { cache: 'reload' })
    this.showcase = /** @type {ShowcasePayload} */ (await response.json())
    this.loading.showcase = false
  }

  /** @returns {Promise<void>} */
  async loadTelemetrySummary() {
    this.loading.telemetry = true
    const response = await this.fetch('/telemetry?limit=6', { cache: 'reload' })
    this.telemetry = /** @type {TelemetryPayload} */ (await response.json())
    this.loading.telemetry = false
  }

  /** @returns {Promise<void>} */
  async addItem() {
    const name = this.newItem.trim()
    if (!name) {
      this.feedback = 'Name is required before saving.'
      return
    }

    const response = await this.fetch('/items', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ name, source: 'dashboard' }),
    })

    const payload = /** @type {{ detail?: string } & Partial<InventoryItem>} */ (await response.json())
    if (!response.ok) {
      this.feedback = payload.detail ?? 'Failed to create item.'
      return
    }

    this.items = [/** @type {InventoryItem} */ (payload), ...this.items]
    this.newItem = ''
    this.feedback = `Saved "${payload.name}" through Yon.`
  }

  /** @returns {Promise<void>} */
  async clearItems() {
    await this.fetch('/items', { method: 'DELETE' })
    this.items = []
    this.feedback = 'Inventory reset.'
  }
}
