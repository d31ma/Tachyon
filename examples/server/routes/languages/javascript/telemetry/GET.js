import TelemetryService from '../../../../services/telemetry-service.js'

const service = new TelemetryService()

/**
 * @param {{ query?: Record<string, unknown> }} request
 * @returns {Promise<{ summary: Record<string, unknown>, recent: Array<Record<string, unknown>> }>}
 */
export async function handler(request) {
  return service.readFromRequest(request)
}
