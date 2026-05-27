import TelemetryService from '@/services/telemetry-service.js'

const service = new TelemetryService()

export class Handler {
  /**
   * @param {{ query?: Record<string, unknown> }} request
   * @returns {Promise<{ summary: Record<string, unknown>, recent: Array<Record<string, unknown>> }>}
   */
  static async GET(request) {
    return service.readFromRequest(request)
  }
}
