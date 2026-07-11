// Streaming save-to-disk via the service worker (see sw.js): chunks are
// piped over a MessagePort into a Response the browser downloads
// incrementally. Returns null when no service worker controls the page
// (dev mode, or the very first visit) — callers fall back to buffering.

export async function openDownloadStream(filename) {
    const sw = navigator.serviceWorker?.controller;
    if (!sw) return null;

    const token = crypto.randomUUID();
    const channel = new MessageChannel();
    const ready = new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error("service worker did not respond")), 3000);
        channel.port1.onmessage = (m) => {
            if (m.data.ready) {
                clearTimeout(timer);
                resolve();
            }
        };
    });
    sw.postMessage({ type: "stream-download", token, filename, port: channel.port2 }, [
        channel.port2,
    ]);
    try {
        await ready;
    } catch {
        return null;
    }

    let cancelled = false;
    channel.port1.onmessage = (m) => {
        if (m.data.cancelled) cancelled = true;
    };

    // Navigating to the tokenized URL starts the download; the relative
    // href keeps it inside the app's mount point (and the SW's scope).
    const a = document.createElement("a");
    a.href = `stream-download/${token}/${encodeURIComponent(filename)}`;
    document.body.appendChild(a);
    a.click();
    a.remove();

    return {
        /** @param {Uint8Array} chunk */
        write(chunk) {
            if (cancelled) return;
            // Copy + transfer: the engine reuses its buffers.
            const buf = chunk.slice().buffer;
            channel.port1.postMessage({ chunk: buf }, [buf]);
        },
        close() {
            channel.port1.postMessage({ done: true });
        },
        abort() {
            channel.port1.postMessage({ abort: true });
        },
    };
}

/** Download filename extension for a tool's declared output type. */
export function extensionFor(outputType) {
    return { text: "txt", json: "json", image: "img", bytes: "bin" }[outputType] ?? "bin";
}
