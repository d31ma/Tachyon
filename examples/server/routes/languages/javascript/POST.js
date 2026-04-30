import LanguageService from '../../../services/language-service.js';

const service = new LanguageService();

/**
 * @typedef {object} YonRequestContext
 * @property {string} [requestId]
 *
 * @typedef {object} YonRequest
 * @property {YonRequestContext} [context]
 * @property {unknown} [body]
 */

/**
 * @param {YonRequest} request
 * @returns {Promise<Record<string, unknown>>}
 */
export async function handler(request) {
  return service.createEcho(request)
}
