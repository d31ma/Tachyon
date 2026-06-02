import RealtimeService from '@/services/realtime-service.js';

export class Handler {
  static #service = new RealtimeService();

  static async GET() {
    return await Handler.#service.listClients();
  }

  /** @param {{ body?: unknown }} request */
  static async POST(request) {
    return await Handler.#service.registerClient(request.body);
  }
}
