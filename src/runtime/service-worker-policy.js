// @ts-check

export class ServiceWorkerPolicy {
    static cachePrefix = 'tachyon-static-';

    static runtimeEntrypoints = new Set([
        '/browser-env.js',
        '/hot-reload-client.js',
        '/imports.js',
        '/spa-renderer.js',
        '/tachyon-sw.js',
    ]);

    static staticAssetPattern = /\.(?:avif|css|gif|ico|jpe?g|js|json|mjs|mp3|mp4|ogg|otf|png|svg|ttf|wasm|webm|webp|woff2?)$/i;

    static staticAssetPrefixes = [
        '/components/',
        '/pages/',
        '/shared/',
        '/workers/',
    ];

    /** @param {string} hostname */
    static isLoopback(hostname) {
        const normalized = hostname.toLowerCase().replace(/^\[|\]$/g, '');
        return normalized === 'localhost'
            || normalized.endsWith('.localhost')
            || normalized === '::1'
            || /^127(?:\.\d{1,3}){3}$/.test(normalized);
    }

    /** @param {{ protocol: string, hostname: string }} location */
    static shouldRegister(location) {
        return (location.protocol === 'http:' || location.protocol === 'https:')
            && !this.isLoopback(location.hostname);
    }

    /**
     * @param {{ method: string, mode: string, destination: string, url: string }} request
     * @param {string} workerOrigin
     * @returns {'bypass' | 'network-first' | 'cache-first'}
     */
    static strategyFor(request, workerOrigin) {
        if (request.method !== 'GET')
            return 'bypass';

        const url = new URL(request.url);
        if (url.origin !== workerOrigin)
            return 'bypass';
        if (url.pathname.startsWith('/_fylo/')
            || url.pathname === '/hmr'
            || url.pathname === '/health'
            || url.pathname === '/ready')
            return 'bypass';
        if (request.mode === 'navigate' || request.destination === 'document')
            return 'network-first';
        if (this.runtimeEntrypoints.has(url.pathname))
            return 'network-first';
        const isBrowserAsset = Boolean(request.destination)
            || this.staticAssetPrefixes.some((prefix) => url.pathname.startsWith(prefix));
        if (isBrowserAsset && this.staticAssetPattern.test(url.pathname))
            return 'cache-first';
        return 'bypass';
    }
}
