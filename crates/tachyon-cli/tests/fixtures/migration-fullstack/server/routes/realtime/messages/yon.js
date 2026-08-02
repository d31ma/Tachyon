import RealtimeService from '@/services/realtime-service.js';

export class Handler {
  static #service = new RealtimeService();

  /** @param {{ body?: unknown }} request */
  static async POST(request) {
    return await Handler.#service.sendMessage(request.body);
  }
}
