// @ts-check
//
// Cache API layer for static assets — JS modules, CSS, WASM workers.
// Complements browser-cache.js (IndexedDB for API responses) with the
// Cache API (purpose-built for Request → Response pairs).
//
// Integrated into tac.load() so every module import is cached for
// instant repeat-visit page loads. The cache is versioned so a new
// app deploy or HMR session automatically replaces stale entries.

const CACHE_NAME = 'tachyon-static-v1';

/** @returns {Promise<Cache>} */
async function openCache() {
    if (!('caches' in window)) throw new Error('Cache API not available');
    return caches.open(CACHE_NAME);
}

/**
 * Cache a module response. Called after a successful network import.
 * @param {string} url
 * @param {Response} response
 */
export async function cacheModuleResponse(url, response) {
    if (!response.ok) return;
    try {
        const cache = await openCache();
        await cache.put(url, response.clone());
    } catch { /* storage full or unavailable */ }
}

/**
 * Check the cache for a previously-stored module response.
 * Returns the cached Response, or null on miss.
 * @param {string} url
 * @returns {Promise<Response | null>}
 */
export async function getCachedModule(url) {
    try {
        const cache = await openCache();
        const cached = await cache.match(url);
        if (!cached) return null;
        return cached;
    } catch {
        return null;
    }
}

/**
 * Drop all cached entries (called on HMR soft-reload so stale modules
 * are evicted and fresh ones are fetched from the server).
 */
export async function clearStaticCache() {
    try {
        await caches.delete(CACHE_NAME);
    } catch { /* best effort */ }
}

/**
 * Pre-cache a set of URLs. Useful after the initial page render to warm
 * the cache with all the modules the page depends on.
 * @param {string[]} urls
 */
export async function precacheModules(urls) {
    try {
        const cache = await openCache();
        await Promise.all(urls.map(async (url) => {
            if (await cache.match(url)) return; // already cached
            try {
                const response = await fetch(url);
                if (response.ok) await cache.put(url, response);
            } catch { /* network unavailable */ }
        }));
    } catch { /* best effort */ }
}
