// @ts-check

export default class LanguageRepository {
    /**
     * @param {{ requestId?: unknown }} [options]
     * @returns {{ frontend: string, backend: string, requestId: string, generatedAt: string }}
     */
    snapshot(options = {}) {
        return {
            frontend: 'Tac',
            backend: 'Yon',
            requestId: String(options.requestId ?? 'unknown'),
            generatedAt: new Date().toISOString(),
        };
    }

    /**
     * @returns {{ name: string, version: string }}
     */
    runtime() {
        return {
            name: 'bun',
            version: Bun.version,
        };
    }
}
