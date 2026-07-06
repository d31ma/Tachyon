// @ts-check
import { ServiceWorkerPolicy } from './service-worker-policy.js';
//
// Tachyon service worker — caches static assets via Cache API, serves
// versioned assets cache-first while navigations and stable runtime
// entrypoints remain network-first. Cache version comes from the registration
// URL: `/tachyon-sw.js?v=<version>`.
//
// Old caches are cleaned on activate. Assets no longer referenced
// by the current deploy naturally expire when their cache version is
// deleted.
//
// The project typechecks against the DOM lib (not WebWorker), so the
// ServiceWorker-specific surface is described with local typedefs and a
// single cast of `self` rather than pulling in a conflicting lib.

/**
 * @typedef {Event & { waitUntil(promise: Promise<unknown>): void }} ExtendableEventLike
 * @typedef {ExtendableEventLike & { request: Request, respondWith(response: Response | Promise<Response>): void }} FetchEventLike
 */

/** @type {{ skipWaiting(): Promise<void>, clients: { claim(): Promise<void> } }} */
const sw = /** @type {any} */ (self);

const CACHE_PREFIX = ServiceWorkerPolicy.cachePrefix;
const VERSION = new URL(self.location.href).searchParams.get('v') || 'v1';
const CACHE_NAME = CACHE_PREFIX + VERSION;

// ── Install ────────────────────────────────────────────────────────────
self.addEventListener('install', (event) => {
    /** @type {ExtendableEventLike} */ (event).waitUntil(sw.skipWaiting());
});

// ── Activate — delete old caches ──────────────────────────────────────
self.addEventListener('activate', (event) => {
    /** @type {ExtendableEventLike} */ (event).waitUntil((async () => {
        const keys = await caches.keys();
        await Promise.all(
            keys
                .filter((key) => key.startsWith(CACHE_PREFIX) && key !== CACHE_NAME)
                .map((key) => caches.delete(key)),
        );
        await sw.clients.claim();
    })());
});

/** @param {Request} request @param {Cache} cache */
async function networkFirst(request, cache) {
    try {
        const response = await fetch(request);
        if (response.ok && response.type === 'basic')
            void cache.put(request, response.clone()).catch(() => {});
        return response;
    } catch {
        return await cache.match(request) || new Response('Offline', { status: 503 });
    }
}

/** @param {Request} request @param {Cache} cache */
async function cacheFirst(request, cache) {
    const cached = await cache.match(request);
    if (cached)
        return cached;
    const response = await fetch(request);
    if (response.ok && response.type === 'basic')
        void cache.put(request, response.clone()).catch(() => {});
    return response;
}

// ── Fetch — explicit strategies for pages and static assets ───────────
self.addEventListener('fetch', (rawEvent) => {
    const event = /** @type {FetchEventLike} */ (rawEvent);
    const strategy = ServiceWorkerPolicy.strategyFor(event.request, self.location.origin);
    if (strategy === 'bypass')
        return;

    event.respondWith((async () => {
        const cache = await caches.open(CACHE_NAME);
        return strategy === 'network-first'
            ? networkFirst(event.request, cache)
            : cacheFirst(event.request, cache);
    })());
});
