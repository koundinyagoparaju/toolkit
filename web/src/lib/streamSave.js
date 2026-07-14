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
                resolve(Boolean(m.data.flow));
            }
        };
    });
    sw.postMessage({ type: "stream-download", token, filename, port: channel.port2 }, [
        channel.port2,
    ]);
    let flowControlled;
    try {
        flowControlled = await ready;
    } catch {
        return null;
    }

    // Backpressure: the worker grants a credit per chunk its stream is
    // willing to queue; write() waits for one, which pauses the engine
    // and, through it, the input file read.
    let cancelled = false;
    let credits = 0;
    const waiters = [];
    channel.port1.onmessage = (m) => {
        if (m.data.pull) {
            const waiter = waiters.shift();
            if (waiter) waiter();
            else credits += 1;
        }
        if (m.data.cancelled) {
            cancelled = true;
            while (waiters.length) waiters.shift()();
        }
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
        async write(chunk) {
            if (cancelled) return;
            if (flowControlled) {
                if (credits === 0) await new Promise((resolve) => waiters.push(resolve));
                if (cancelled) return;
                credits -= 1;
            }
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
