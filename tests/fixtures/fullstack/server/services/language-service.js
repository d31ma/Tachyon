// @ts-check

import LanguageRepository from '../repositories/language-repository.js';
import FyloMachineRepository from '../repositories/fylo-machine-repository.js';

export default class LanguageService {
    /**
     * @param {{ repository?: LanguageRepository, fyloRepository?: FyloMachineRepository }} [options]
     */
    constructor(options = {}) {
        this.repository = options.repository ?? new LanguageRepository();
        this.fyloRepository = options.fyloRepository ?? new FyloMachineRepository();
    }

    /**
     * @param {Record<string, any>} request
     * @returns {Record<string, unknown>}
     */
    diagnostics(request) {
        return {
            message: 'Hello from Yon on Bun!',
            runtime: this.repository.runtime(),
            context: request.context,
            frontend: 'Tac',
            backend: 'Yon',
        };
    }

    /**
     * Drives the full FYLO machine interface from a dedicated route. Heavy
     * (≈25 subprocess spawns) and only meant to be invoked deliberately —
     * it is intentionally NOT part of the lightweight diagnostics response.
     *
     * @param {Record<string, any>} request
     * @returns {Promise<Record<string, unknown>>}
     */
    async fyloDemo(request) {
        return await this.fyloRepository.demonstrateAll(String(request.context?.requestId ?? 'unknown'));
    }

    /**
     * @param {Record<string, any>} request
     * @returns {Record<string, unknown>}
     */
    createEcho(request) {
        return {
            message: 'Hello from Yon language service!',
            snapshot: this.repository.snapshot({ requestId: request.context?.requestId }),
        };
    }

    /**
     * @param {Record<string, any>} request
     * @returns {Record<string, unknown>}
     */
    updateEcho(request) {
        return {
            message: 'Hello from Yon on Bun!',
            method: 'PUT',
            context: request.context,
            snapshot: this.repository.snapshot({ requestId: request.context?.requestId }),
        };
    }

}
