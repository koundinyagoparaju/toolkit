// Generic loader for the pack ABI. Every pack exports the same four
// functions, so this one file talks to all packs, present and future
// (including packs written in languages other than Rust).
//
// Framing (mirrors crates/core/src/abi.rs):
//   [u32 LE header length][header JSON][payload bytes]

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** module name -> Promise<exports>, so each pack is fetched at most once. */
const packs = new Map();

export function loadPack(module) {
    if (!packs.has(module)) {
        packs.set(
            module,
            WebAssembly.instantiateStreaming(fetch(`wasm/${module}`), {}).then(
                (result) => result.instance.exports,
            ),
        );
    }
    return packs.get(module);
}

function frame(header, payload) {
    const headerBytes = encoder.encode(JSON.stringify(header));
    const buf = new Uint8Array(4 + headerBytes.length + payload.length);
    new DataView(buf.buffer).setUint32(0, headerBytes.length, true);
    buf.set(headerBytes, 4);
    buf.set(payload, 4 + headerBytes.length);
    return buf;
}

function unframe(buf) {
    const headerLen = new DataView(buf.buffer, buf.byteOffset, 4).getUint32(0, true);
    const header = JSON.parse(decoder.decode(buf.subarray(4, 4 + headerLen)));
    const payload = buf.subarray(4 + headerLen).slice();
    return { header, payload };
}

/** Copy a (ptr << 32 | len)-packed buffer out of wasm memory and free it. */
function takeBuffer(exports, packed) {
    const ptr = Number(packed >> 32n);
    const len = Number(packed & 0xffffffffn);
    const bytes = new Uint8Array(exports.memory.buffer, ptr, len).slice();
    exports.tk_dealloc(ptr, len);
    return bytes;
}

/**
 * Run one tool in a pack.
 * @param {string} module - pack module file, e.g. "text.wasm"
 * @param {string} tool - tool name, e.g. "base64-decode"
 * @param {object} options - option values
 * @param {Object.<string, object|object[]>} inputs - per input port: a
 *   {type, bytes, format?} value, or an ordered array of them (multi ports)
 * @returns the output value in the same shape
 * @throws Error with the tool's message on failure
 */
export async function runTool(module, tool, options, inputs) {
    const exports = await loadPack(module);
    const entries = Object.entries(inputs).flatMap(([port, v]) =>
        (Array.isArray(v) ? v : [v]).map((value) => [port, value]),
    );
    const requestHeader = {
        tool,
        options,
        inputs: entries.map(([port, value]) => {
            const meta = { port, type: value.type, len: value.bytes.length };
            if (value.format) meta.format = value.format;
            return meta;
        }),
    };
    const total = entries.reduce((n, [, v]) => n + v.bytes.length, 0);
    const requestPayload = new Uint8Array(total);
    let offset = 0;
    for (const [, value] of entries) {
        requestPayload.set(value.bytes, offset);
        offset += value.bytes.length;
    }
    const request = frame(requestHeader, requestPayload);

    const ptr = exports.tk_alloc(request.length);
    new Uint8Array(exports.memory.buffer, ptr, request.length).set(request);
    const packed = exports.tk_run(ptr, request.length);
    exports.tk_dealloc(ptr, request.length);

    const { header, payload } = unframe(takeBuffer(exports, packed));
    if (!header.ok) throw new Error(header.error ?? "tool failed");
    return { type: header.output.type, format: header.output.format, bytes: payload };
}

/** Read a pack's own manifest list (mainly useful for debugging/audit). */
export async function packManifests(module) {
    const exports = await loadPack(module);
    const bytes = takeBuffer(exports, exports.tk_manifests());
    return JSON.parse(decoder.decode(bytes));
}
