import LanguageService from '@/services/language-service.js';

export class Handler {
  static #service = new LanguageService();

  /**
   * Drives the FYLO machine interface end-to-end via `fylo.exec` for the
   * dashboard's polyglot showcase. Heavy (approximately 25 subprocess spawns) — kept on a
   * dedicated route so it doesn't slow down the lightweight `/languages/javascript`
   * diagnostics endpoint or interfere with other example collections.
   *
   * @param {{ context?: { requestId?: string } }} request
   * @returns {Promise<Record<string, unknown>>}
   */
  static async POST(request) {
    return await Handler.#service.fyloDemo(request);
  }
}
