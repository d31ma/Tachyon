// @ts-check

import LanguageRepository from '../repositories/language-repository.js';

export default class LanguageService {
    /**
     * @param {{ repository?: LanguageRepository }} [options]
     */
    constructor(options = {}) {
        this.repository = options.repository ?? new LanguageRepository();
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
