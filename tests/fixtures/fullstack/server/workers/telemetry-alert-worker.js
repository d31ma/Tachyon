#!/usr/bin/env bun

/**
 * @typedef {{
 *   traceId: string,
 *   spanId: string,
 *   parentSpanId: string,
 *   kind: string,
 *   name: string,
 *   requestId: string | null,
 *   route: string | null,
 *   method: string | null,
 *   statusCode: number | null,
 *   traceState: string,
 *   startTimeUnixNano: string,
 *   endTimeUnixNano: string,
 *   durationMs: number,
 * }} TelemetryEntry
 *
 * @typedef {{
 *   summary: {
 *     enabled: boolean,
 *     collection: string,
 *     spanCount: number,
 *     requestCount: number,
 *     errorCount: number,
 *   },
 *   recent: TelemetryEntry[],
 * }} TelemetryPayload
 *
 * @typedef {{
 *   level: 'warn' | 'error',
 *   reason: 'slow-route' | 'error-route',
 *   route: string,
 *   method: string,
 *   statusCode: number | null,
 *   durationMs: number,
 *   traceId: string,
 *   requestId: string | null,
 * }} TelemetryAlert
 */

class TelemetryAlertWorker {
  constructor() {
    this.telemetryUrl = process.env.YON_TELEMETRY_URL || 'http://127.0.0.1:8000/languages/javascript/telemetry?limit=25'
    this.slowMs = TelemetryAlertWorker.numberEnv('YON_ALERT_SLOW_MS', 500)
    this.minStatusCode = TelemetryAlertWorker.numberEnv('YON_ALERT_STATUS_CODE', 500)
    this.authorization = process.env.YON_BASIC_AUTH_HEADER || ''
  }

  /**
   * @param {string} name
   * @param {number} fallback
   * @returns {number}
   */
  static numberEnv(name, fallback) {
    const value = Number(process.env[name] || '')
    return Number.isFinite(value) && value > 0 ? value : fallback
  }

  /**
   * @returns {Promise<void>}
   */
  async run() {
    const payload = await this.fetchTelemetry()
    const alerts = this.buildAlerts(payload.recent)
    const output = {
      checkedAt: new Date().toISOString(),
      telemetryUrl: this.telemetryUrl,
      thresholds: {
        slowMs: this.slowMs,
        minStatusCode: this.minStatusCode,
      },
      summary: payload.summary,
      alertCount: alerts.length,
      alerts,
    }
    Bun.stdout.write(`${JSON.stringify(output, null, 2)}\n`)
  }

  /**
   * @returns {Promise<TelemetryPayload>}
   */
  async fetchTelemetry() {
    const headers = new Headers()
    if (this.authorization) {
      headers.set('Authorization', this.authorization)
    }
    const response = await fetch(this.telemetryUrl, { headers })
    if (!response.ok) {
      throw new Error(`Telemetry endpoint returned ${response.status}`)
    }
    return /** @type {Promise<TelemetryPayload>} */ (response.json())
  }

  /**
   * @param {TelemetryEntry[]} entries
   * @returns {TelemetryAlert[]}
   */
  buildAlerts(entries) {
    /** @type {TelemetryAlert[]} */
    const alerts = []
    for (const entry of entries) {
      if (entry.kind !== 'server') {
        continue
      }
      if (typeof entry.statusCode === 'number' && entry.statusCode >= this.minStatusCode) {
        alerts.push({
          level: 'error',
          reason: 'error-route',
          route: entry.route || entry.name,
          method: entry.method || 'GET',
          statusCode: entry.statusCode,
          durationMs: entry.durationMs,
          traceId: entry.traceId,
          requestId: entry.requestId,
        })
        continue
      }
      if (entry.durationMs >= this.slowMs) {
        alerts.push({
          level: 'warn',
          reason: 'slow-route',
          route: entry.route || entry.name,
          method: entry.method || 'GET',
          statusCode: entry.statusCode,
          durationMs: entry.durationMs,
          traceId: entry.traceId,
          requestId: entry.requestId,
        })
      }
    }
    return alerts
  }
}

const worker = new TelemetryAlertWorker()
await worker.run()
