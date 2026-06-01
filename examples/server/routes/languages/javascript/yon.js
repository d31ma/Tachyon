import LanguageService from '@/services/language-service.js';

export class Handler {
  static #service = new LanguageService();

  /** @type {Record<string, Record<string, unknown>>} */
  static #statusResponses = {
    '200': { code: '200', message: 'ok' },
    '201': { code: '201', message: 'created' },
    '202': { code: '202', message: 'accepted' },
    '203': { code: '203', message: 'non-authoritative' },
    '205': { code: '205', message: 'reset content' },
    '206': { code: '206', total: 1, message: 'partial' },
  };

  /**
   * @param {{ context?: { requestId?: string }, query?: Record<string, unknown> }} request
   * @returns {Promise<Record<string, unknown>>}
   */
  static async GET(request) {
    const code = Handler.#statusCode(request);
    if (code && Handler.#statusResponses[code])
      return Handler.#statusResponses[code];
    return Handler.#service.diagnostics(request);
  }

  /**
   * @param {{ context?: { requestId?: string }, body?: unknown }} request
   * @returns {Promise<Record<string, unknown>>}
   */
  static async POST(request) {
    return Handler.#service.createEcho(request);
  }

  /**
   * @param {{ context?: { requestId?: string }, body?: unknown }} request
   * @returns {Promise<Record<string, unknown>>}
   */
  static async PUT(request) {
    return Handler.#service.updateEcho(request);
  }

  /**
   * @param {{ query?: Record<string, unknown> }} request
   * @returns {string}
   */
  static #statusCode(request) {
    const raw = request.query?.code;
    if (typeof raw === 'number') return String(Math.floor(raw));
    return typeof raw === 'string' ? raw : '';
  }
}
