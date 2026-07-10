// Loads the tool catalog (manifests.json, generated from the Rust core) and
// the community chain library. Fetched once; both are same-origin static
// files, cached offline by the service worker.

const encoder = new TextEncoder();
const decoder = new TextDecoder();

let catalogPromise;

export function loadCatalog() {
    catalogPromise ??= fetch("wasm/manifests.json")
        .then((r) => r.json())
        .then((raw) => {
            const tools = new Map();
            for (const pack of raw.packs) {
                for (const manifest of pack.tools) {
                    tools.set(manifest.name, { ...manifest, pack: pack.name, module: pack.module });
                }
            }
            return { version: raw.version, coercions: raw.coercions, packs: raw.packs, tools };
        });
    return catalogPromise;
}

let chainsPromise;

export function loadChainLibrary() {
    chainsPromise ??= fetch("chains/index.json")
        .then((r) => r.json())
        .then((files) =>
            Promise.all(
                files.map((f) =>
                    fetch(`chains/${f}`)
                        .then((r) => r.json())
                        .then((chain) => ({ file: f, ...chain })),
                ),
            ),
        );
    return chainsPromise;
}

/** Can a value of type `from` feed an input of type `to`? */
export function typeCompatible(coercions, from, to) {
    return (coercions[from] ?? []).includes(to);
}

// ---- value helpers (the JS twin of DataValue payloads) ----

export function textValue(text) {
    return { type: "text", bytes: encoder.encode(text) };
}

export function bytesValue(bytes) {
    return { type: "bytes", bytes };
}

export function valueText(value) {
    return decoder.decode(value.bytes);
}

export function prettySize(n) {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}

/** Serialize a chain to a URL-safe string (for share links). */
export function chainToHash(chain) {
    const json = JSON.stringify(chain);
    return btoa(String.fromCharCode(...encoder.encode(json)))
        .replaceAll("+", "-")
        .replaceAll("/", "_")
        .replaceAll("=", "");
}

export function chainFromHash(hash) {
    const b64 = hash.replaceAll("-", "+").replaceAll("_", "/");
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    return JSON.parse(decoder.decode(bytes));
}

/** Materialize a value's bytes (large files defer reading until needed). */
export async function ensureBytes(value) {
    if (value.bytes) return value;
    const bytes = new Uint8Array(await value.file.arrayBuffer());
    return { ...value, bytes };
}

/** Stream a File as an async iterable of Uint8Array chunks. */
export async function* fileChunks(file) {
    const reader = file.stream().getReader();
    for (;;) {
        const { done, value } = await reader.read();
        if (done) return;
        yield value;
    }
}

export function concatBytes(chunks) {
    const total = chunks.reduce((n, c) => n + c.length, 0);
    const out = new Uint8Array(total);
    let offset = 0;
    for (const c of chunks) {
        out.set(c, offset);
        offset += c.length;
    }
    return out;
}
