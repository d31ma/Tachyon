import LanguageService from '@/services/language-service.js';

const service = new LanguageService();

/** @type {Record<string, Record<string, unknown>>} */
const statusResponses = {
  '200': { code: '200', message: 'ok' },
  '201': { code: '201', message: 'created' },
  '202': { code: '202', message: 'accepted' },
  '203': { code: '203', message: 'non-authoritative' },
  '205': { code: '205', message: 'reset content' },
  '206': { code: '206', total: 1, message: 'partial' },
};

/**
 * @typedef {object} YonRequestContext
 * @property {string} [requestId]
 *
 * @typedef {object} YonRequest
 * @property {YonRequestContext} [context]
 * @property {Record<string, unknown>} [query]
 */

/**
 * @param {YonRequest} request
 * @returns {Promise<Record<string, unknown>>}
 */
export async function handler(request) {
  const raw = request.query?.code;
  const code = typeof raw === 'number' ? String(Math.floor(raw)) : typeof raw === 'string' ? raw : '';
  if (code && statusResponses[code])
    return statusResponses[code];
  return service.diagnostics(request)
}
