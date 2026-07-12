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

// Streaming downloads: the page registers a token + filename and pipes
// chunks over a MessagePort; fetching stream-download/<token>/<name>
// returns a Response whose body is that live stream. The browser's
// download manager writes it to disk incrementally, so a chain sink can
// produce gigabytes without the page ever holding them in memory.
const downloads = new Map(); // token -> {stream, filename}

self.addEventListener("message", (event) => {
    const data = event.data;
    if (data?.type !== "stream-download") return;
    const port = data.port;
    const stream = new ReadableStream({
        start(controller) {
            port.onmessage = (m) => {
                try {
                    if (m.data.chunk) controller.enqueue(new Uint8Array(m.data.chunk));
                    if (m.data.done) controller.close();
                    if (m.data.abort) controller.error(new Error("aborted by the page"));
                } catch {
                    // Stream already closed/cancelled (e.g. user cancelled
                    // the download): drop further chunks silently.
                }
                if (m.data.done || m.data.abort) port.onmessage = null;
            };
        },
        cancel() {
            port.postMessage({ cancelled: true });
        },
    });
    downloads.set(data.token, { stream, filename: data.filename });
    port.postMessage({ ready: true });
});

self.addEventListener("fetch", (event) => {
    const url = new URL(event.request.url);
    if (event.request.method !== "GET" || url.origin !== location.origin) return;
    const segments = url.pathname.split("/");
    if (segments.length >= 3 && segments[segments.length - 3] === "stream-download") {
        const token = segments[segments.length - 2];
        const entry = downloads.get(token);
        if (entry) {
            downloads.delete(token);
            event.respondWith(
                new Response(entry.stream, {
                    headers: {
                        "Content-Type": "application/octet-stream",
                        "Content-Disposition": `attachment; filename="${entry.filename.replaceAll('"', "")}"`,
                        "X-Content-Type-Options": "nosniff",
                    },
                }),
            );
            return;
        }
    }
    event.respondWith(
        caches.open(CACHE).then(async (cache) => {
            const cached = await cache.match(event.request);
            const refresh = fetch(event.request)
                .then((response) => {
                    if (response.ok) cache.put(event.request, response.clone());
                    return response;
                })
                .catch(() => cached);
            // fetch(url, {cache: "reload"}) is a deliberate bypass — the
            // wasm loader re-fetching after an integrity mismatch. Serve
            // from the network (still updating the cache); fall back to
            // the cache only if the network is unreachable.
            if (event.request.cache === "reload") return refresh;
            return cached || refresh;
        }),
    );
});
