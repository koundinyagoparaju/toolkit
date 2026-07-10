// Offline support: stale-while-revalidate over same-origin GETs.
// First visit caches the app shell; wasm packs are cached as they are
// lazy-loaded. After that the whole app works in airplane mode.
const CACHE = "toolkit-v1";

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => {
    event.waitUntil(
        caches
            .keys()
            .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
            .then(() => self.clients.claim()),
    );
});

self.addEventListener("fetch", (event) => {
    const url = new URL(event.request.url);
    if (event.request.method !== "GET" || url.origin !== location.origin) return;
    event.respondWith(
        caches.open(CACHE).then(async (cache) => {
            const cached = await cache.match(event.request);
            const refresh = fetch(event.request)
                .then((response) => {
                    if (response.ok) cache.put(event.request, response.clone());
                    return response;
                })
                .catch(() => cached);
            return cached || refresh;
        }),
    );
});
