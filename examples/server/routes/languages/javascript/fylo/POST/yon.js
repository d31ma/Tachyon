import LanguageService from '@/services/language-service.js';

const service = new LanguageService();

/**
 * @typedef {object} YonRequestContext
 * @property {string} [requestId]
 *
 * @typedef {object} YonRequest
 * @property {YonRequestContext} [context]
 */

/**
 * Drives the FYLO machine interface end-to-end via `fylo.exec` for the
 * dashboard's polyglot showcase. Heavy (≈25 subprocess spawns) — kept on a
 * dedicated route so it doesn't slow down the lightweight `/languages/javascript`
 * diagnostics endpoint or interfere with other example collections.
 *
 * @param {YonRequest} request
 * @returns {Promise<Record<string, unknown>>}
 */
export async function handler(request) {
  return await service.fyloDemo(request);
}
