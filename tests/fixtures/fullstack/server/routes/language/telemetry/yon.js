import TelemetryService from '@/services/telemetry-service.js'

export class Handler {
  static #service = new TelemetryService()

  /**
   * @param {{ query?: Record<string, unknown> }} request
   * @returns {Promise<{ summary: Record<string, unknown>, recent: Array<Record<string, unknown>> }>}
   */
  static async GET(request) {
    return Handler.#service.readFromRequest(request)
  }
}
