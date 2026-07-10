// Exercises the built wasm packs through the pack ABI — the exact byte
// protocol the web loader speaks (docs/architecture.md). Run from the repo
// root after ./scripts/build-web-assets.sh:  node tests/abi.test.mjs
import { readFile } from "node:fs/promises";

const enc = new TextEncoder();
const dec = new TextDecoder();

function frame(header, payload) {
    const h = enc.encode(JSON.stringify(header));
    const buf = new Uint8Array(4 + h.length + payload.length);
    new DataView(buf.buffer).setUint32(0, h.length, true);
    buf.set(h, 4);
    buf.set(payload, 4 + h.length);
    return buf;
}

function unframe(buf) {
    const len = new DataView(buf.buffer, buf.byteOffset, 4).getUint32(0, true);
    return {
        header: JSON.parse(dec.decode(buf.subarray(4, 4 + len))),
        payload: buf.subarray(4 + len).slice(),
    };
}

function takeBuffer(ex, packed) {
    const ptr = Number(packed >> 32n);
    const len = Number(packed & 0xffffffffn);
    const bytes = new Uint8Array(ex.memory.buffer, ptr, len).slice();
    ex.tk_dealloc(ptr, len);
    return bytes;
}

async function load(path) {
    const { instance } = await WebAssembly.instantiate(await readFile(path), {});
    return instance.exports;
}

function run(ex, tool, options, inputs) {
    const entries = Object.entries(inputs).flatMap(([port, v]) =>
        (Array.isArray(v) ? v : [v]).map((value) => [port, value]),
    );
    const header = {
        tool,
        options,
        inputs: entries.map(([port, v]) => ({
            port,
            type: v.type,
            ...(v.format ? { format: v.format } : {}),
            len: v.bytes.length,
        })),
    };
    const total = entries.reduce((n, [, v]) => n + v.bytes.length, 0);
    const payload = new Uint8Array(total);
    let offset = 0;
    for (const [, v] of entries) {
        payload.set(v.bytes, offset);
        offset += v.bytes.length;
    }
    const req = frame(header, payload);
    const ptr = ex.tk_alloc(req.length);
    new Uint8Array(ex.memory.buffer, ptr, req.length).set(req);
    const packed = ex.tk_run(ptr, req.length);
    ex.tk_dealloc(ptr, req.length);
    const res = unframe(takeBuffer(ex, packed));
    if (!res.header.ok) throw new Error(res.header.error);
    return { ...res.header.output, bytes: res.payload };
}

const sole = (type, bytes) => ({ input: { type, bytes } });
const assert = (cond, msg) => {
    if (!cond) throw new Error(`FAIL: ${msg}`);
    console.log(`ok - ${msg}`);
};

// ---- text pack ----
const text = await load("web/public/wasm/text.wasm");

const manifests = JSON.parse(dec.decode(takeBuffer(text, text.tk_manifests())));
assert(manifests.length === 21, `text pack reports 21 manifests (got ${manifests.length})`);
assert(
    manifests.every((m) => Array.isArray(m.inputs) && m.inputs.length >= 1),
    "every manifest declares input ports",
);

const b64 = run(text, "base64-encode", {}, sole("text", enc.encode("hello wasm")));
assert(dec.decode(b64.bytes) === "aGVsbG8gd2FzbQ==", "base64-encode via ABI");

const back = run(text, "base64-decode", {}, sole("text", b64.bytes));
assert(dec.decode(back.bytes) === "hello wasm", "base64-decode round trip");

const picked = run(
    text,
    "json-pick",
    { path: "a.1.b" },
    sole("json", enc.encode('{"a":[{},{"b":42}]}')),
);
assert(dec.decode(picked.bytes) === "42", "json-pick extracts nested value");


const mergedDocs = run(
    text,
    "doc-merge",
    { separator: " / " },
    { documents: [
        { type: "text", bytes: enc.encode("one") },
        { type: "text", bytes: enc.encode("two") },
        { type: "text", bytes: enc.encode("three") },
    ] },
);
assert(dec.decode(mergedDocs.bytes) === "one / two / three", "doc-merge joins 3 docs in order via ABI");

let tooMany = false;
try {
    run(text, "base64-encode", {}, { input: [
        { type: "text", bytes: enc.encode("a") },
        { type: "text", bytes: enc.encode("b") },
    ] });
} catch (e) {
    tooMany = e.message.includes("one value");
}
assert(tooMany, "two values on a single port rejected via ABI");

let failed = false;
try {
    run(text, "base64-decode", {}, sole("text", enc.encode("!!!")));
} catch (e) {
    failed = e.message.includes("not valid base64");
}
assert(failed, "tool error surfaces as ABI error response");

// ---- image pack ----
const image = await load("web/public/wasm/image.wasm");

// 1x1 red PNG.
const png = Uint8Array.from(
    atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="),
    (c) => c.charCodeAt(0),
);
const resized = run(
    image,
    "image-resize",
    { width: 8, height: 8, mode: "exact" },
    sole("bytes", png), // bytes -> image coercion happens pack-side
);
assert(resized.type === "image" && resized.format === "png", "resize returns a png image value");
const dv = new DataView(resized.bytes.buffer, resized.bytes.byteOffset);
assert(dv.getUint32(16) === 8 && dv.getUint32(20) === 8, "resized image is 8x8");

// Multi-port: merge two images through two named ports.
const merged = run(
    image,
    "image-merge",
    { mode: "horizontal" },
    {
        first: { type: "image", format: "png", bytes: resized.bytes },
        second: { type: "bytes", bytes: png },
    },
);
const mdv = new DataView(merged.bytes.buffer, merged.bytes.byteOffset);
assert(
    mdv.getUint32(16) === 9 && mdv.getUint32(20) === 8,
    `merge(8x8, 1x1) horizontal is 9x8 (got ${mdv.getUint32(16)}x${mdv.getUint32(20)})`,
);

let missing = false;
try {
    run(image, "image-merge", {}, { first: { type: "bytes", bytes: png } });
} catch (e) {
    missing = e.message.includes("second");
}
assert(missing, "missing port produces a clear ABI error");


// ---- streaming ABI ----
function openStream(ex, tool, options) {
    const req = frame({ tool, options }, new Uint8Array(0));
    const ptr = ex.tk_alloc(req.length);
    new Uint8Array(ex.memory.buffer, ptr, req.length).set(req);
    const packed = ex.tk_stream_open(ptr, req.length);
    ex.tk_dealloc(ptr, req.length);
    const res = unframe(takeBuffer(ex, packed));
    if (!res.header.ok) throw new Error(res.header.error);
    return res.header.handle;
}
function streamCall(ex, fn, handle, port, index, chunk) {
    const req = frame({ port, index }, chunk ?? new Uint8Array(0));
    const ptr = ex.tk_alloc(req.length);
    new Uint8Array(ex.memory.buffer, ptr, req.length).set(req);
    const packed = fn(handle, ptr, req.length);
    ex.tk_dealloc(ptr, req.length);
    const res = unframe(takeBuffer(ex, packed));
    if (!res.header.ok) throw new Error(res.header.error);
    return res.payload;
}
function streamFinish(ex, handle) {
    const res = unframe(takeBuffer(ex, ex.tk_stream_finish(handle)));
    if (!res.header.ok) throw new Error(res.header.error);
    return res.payload;
}

const h = openStream(text, "hash", {});
streamCall(text, text.tk_stream_update.bind(text), h, "input", 0, enc.encode("a"));
streamCall(text, text.tk_stream_update.bind(text), h, "input", 0, enc.encode("bc"));
streamCall(text, text.tk_stream_end_input.bind(text), h, "input", 0);
const streamedDigest = dec.decode(streamFinish(text, h));
assert(
    streamedDigest === "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    "streamed hash via ABI sessions matches known vector",
);

const h2 = openStream(text, "doc-merge", { separator: "|" });
let merged2 = [];
merged2.push(streamCall(text, text.tk_stream_update.bind(text), h2, "documents", 1, enc.encode("late")));
merged2.push(streamCall(text, text.tk_stream_update.bind(text), h2, "documents", 0, enc.encode("first")));
merged2.push(streamCall(text, text.tk_stream_end_input.bind(text), h2, "documents", 0));
merged2.push(streamCall(text, text.tk_stream_end_input.bind(text), h2, "documents", 1));
merged2.push(streamFinish(text, h2));
const merged2Text = merged2.map((b) => dec.decode(b)).join("");
assert(merged2Text === "first|late", `streamed doc-merge orders out-of-order input (got "${merged2Text}")`);

let badHandle = false;
try { streamFinish(text, 99); } catch (e) { badHandle = e.message.includes("invalid stream handle"); }
assert(badHandle, "invalid stream handle is a clean error");


// ---- crypto pack: generator with driver-supplied entropy ----
const cryptoPack = await load("web/public/wasm/crypto.wasm");
const fixedEntropy = new Uint8Array(1024).fill(0xab);
const uuid1 = run(cryptoPack, "uuid", {}, { entropy: { type: "bytes", bytes: fixedEntropy } });
const uuid2 = run(cryptoPack, "uuid", {}, { entropy: { type: "bytes", bytes: fixedEntropy } });
const uuidText = dec.decode(uuid1.bytes);
assert(
    /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(uuidText),
    `uuid via ABI is well-formed v4 (got ${uuidText})`,
);
assert(dec.decode(uuid2.bytes) === uuidText, "uuid is a pure function of its entropy");

const hm = run(cryptoPack, "hmac", {}, {
    key: { type: "text", bytes: enc.encode("key") },
    message: { type: "text", bytes: enc.encode("The quick brown fox jumps over the lazy dog") },
});
assert(
    dec.decode(hm.bytes) === "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8",
    "hmac-sha256 known vector via ABI",
);

// ---- data pack ----
const dataPack = await load("web/public/wasm/data.wasm");
const yaml = run(dataPack, "yaml-to-json", {}, { input: { type: "text", bytes: enc.encode("a: 1\nb: [x, y]") } });
assert(dec.decode(yaml.bytes) === '{"a":1,"b":["x","y"]}', "yaml-to-json via ABI");

console.log("\nAll ABI tests passed.");
