// Client-side chain execution: a push-based dataflow mirroring the Rust
// engine (crates/core/src/chain.rs). Streaming tools transform chunk by
// chunk through wasm sessions; non-streaming tools buffer at their inputs
// ("reservoirs") and run once complete. Buffered execution is the same
// engine fed a single chunk, so the two modes cannot diverge.

import { typeCompatible } from "./catalog.js";
import { openToolStream, runTool } from "./wasm.js";

const PREVIEW_CAP = 4096;
const ENTROPY_LEN = 1024;

/** Resolve which input port an edge feeds, mirroring core's resolve_port. */
export function edgePort(edge, toTool) {
    if (edge.to_port) {
        const port = toTool.inputs.find((p) => p.name === edge.to_port);
        if (!port) throw new Error(`"${toTool.name}" has no input port "${edge.to_port}"`);
        return port;
    }
    if (toTool.inputs.length !== 1) {
        throw new Error(`"${toTool.name}" has ${toTool.inputs.length} input ports; the edge must name one`);
    }
    return toTool.inputs[0];
}

/** Resolve which port an input binding feeds (mirrors core). */
export function bindPort(bind, tool) {
    if (bind.port) {
        const port = tool.inputs.find((p) => p.name === bind.port);
        if (!port) throw new Error(`"${tool.name}" has no input port "${bind.port}"`);
        return port;
    }
    if (tool.inputs.length !== 1) {
        throw new Error(`"${tool.name}" has ${tool.inputs.length} input ports; the binding must name one`);
    }
    return tool.inputs[0];
}

/** Apply declared chain params (mirrors core's with_params, minus deep
 *  validation — the Rust side re-validates every option before running). */
export function applyParams(chain, values) {
    const nodes = chain.nodes.map((n) => ({ ...n, options: { ...(n.options ?? {}) } }));
    for (const param of chain.params ?? []) {
        const value = values[param.name] ?? param.default;
        if (value === undefined || value === "") continue;
        for (const target of param.maps) {
            const node = nodes.find((n) => n.id === target.node);
            if (!node) throw new Error(`param "${param.name}" maps to unknown node "${target.node}"`);
            node.options[target.option] = value;
        }
    }
    return { ...chain, nodes };
}

/** @returns {string[]} node ids in execution order @throws on invalid DAG */
export function validateChain(chain, catalog) {
    if (!chain.nodes?.length) throw new Error("chain has no nodes");
    const seen = new Set();
    for (const node of chain.nodes) {
        if (seen.has(node.id)) throw new Error(`duplicate node id "${node.id}"`);
        seen.add(node.id);
        if (!catalog.tools.has(node.tool)) throw new Error(`unknown tool "${node.tool}"`);
    }

    const wired = new Set(); // "node port"
    const incoming = new Map();
    for (const edge of chain.edges ?? []) {
        const from = chain.nodes.find((n) => n.id === edge.from);
        const to = chain.nodes.find((n) => n.id === edge.to);
        if (!from || !to) throw new Error("edge references a missing node");
        const fromTool = catalog.tools.get(from.tool);
        const toTool = catalog.tools.get(to.tool);
        const port = edgePort(edge, toTool);
        if (!typeCompatible(catalog.coercions, fromTool.output, port.type)) {
            throw new Error(
                `"${from.tool}" outputs ${fromTool.output}, which cannot feed the ${port.type} port "${port.name}" of "${to.tool}"`,
            );
        }
        const key = `${edge.to} ${port.name}`;
        if (wired.has(key) && !port.multi) {
            throw new Error(`input port "${port.name}" of "${edge.to}" has more than one incoming edge`);
        }
        wired.add(key);
        incoming.set(edge.to, (incoming.get(edge.to) ?? 0) + 1);
    }

    // Declared inputs: unique names, resolvable binds, each port fed by at
    // most one kind of source (mirrors core's rules).
    const inputBound = new Set(); // "node port"
    const inputNames = new Set();
    for (const input of chain.inputs ?? []) {
        if (!input.name) throw new Error("chain input with an empty name");
        if (inputNames.has(input.name)) throw new Error(`duplicate chain input "${input.name}"`);
        inputNames.add(input.name);
        if (!input.binds?.length) throw new Error(`chain input "${input.name}" binds no ports`);
        for (const bind of input.binds) {
            const node = chain.nodes.find((n) => n.id === bind.node);
            if (!node) throw new Error(`input "${input.name}" binds unknown node "${bind.node}"`);
            const port = bindPort(bind, catalog.tools.get(node.tool));
            if (port.entropy) {
                throw new Error(`input "${input.name}" binds entropy port "${port.name}" — the driver fills entropy`);
            }
            const key = `${bind.node} ${port.name}`;
            if (wired.has(key)) {
                throw new Error(`port "${port.name}" of "${bind.node}" is fed by both an edge and input "${input.name}"`);
            }
            if (inputBound.has(key) && !port.multi) {
                throw new Error(`input port "${port.name}" of "${bind.node}" is bound more than once`);
            }
            inputBound.add(key);
        }
    }

    // Port coverage: without declared inputs, a node with any incoming edge
    // must have every port wired (nodes with none are entries); with them,
    // every non-entropy port needs an edge or a binding.
    const declared = (chain.inputs ?? []).length > 0;
    for (const node of chain.nodes) {
        if (!declared && !incoming.get(node.id)) continue;
        const tool = catalog.tools.get(node.tool);
        const missing = tool.inputs.filter(
            (p) => !p.entropy && !wired.has(`${node.id} ${p.name}`) && !inputBound.has(`${node.id} ${p.name}`),
        );
        if (missing.length) {
            throw new Error(
                `node "${node.id}": input port(s) not connected: ${missing.map((p) => p.name).join(", ")}`,
            );
        }
    }

    // Kahn's algorithm, ties broken by declaration order (same as core).
    const order = [];
    const degree = new Map(chain.nodes.map((n) => [n.id, incoming.get(n.id) ?? 0]));
    const queue = chain.nodes.filter((n) => degree.get(n.id) === 0).map((n) => n.id);
    while (queue.length) {
        const id = queue.shift();
        order.push(id);
        for (const edge of (chain.edges ?? []).filter((e) => e.from === id)) {
            degree.set(edge.to, degree.get(edge.to) - 1);
            if (degree.get(edge.to) === 0) queue.push(edge.to);
        }
    }
    if (order.length !== chain.nodes.length) throw new Error("chain contains a cycle");
    return order;
}

function concat(chunks) {
    const total = chunks.reduce((n, c) => n + c.length, 0);
    const out = new Uint8Array(total);
    let offset = 0;
    for (const c of chunks) {
        out.set(c, offset);
        offset += c.length;
    }
    return out;
}

/** Build the engine's wiring: per-node slots (port, index, meta) grouped by
 *  port in edge-declaration order, plus each node's outgoing slot list. */
function buildEngine(chain, catalog) {
    const idx = new Map(chain.nodes.map((n, i) => [n.id, i]));
    const nodes = chain.nodes.map((n) => {
        const tool = catalog.tools.get(n.tool);
        return {
            id: n.id,
            tool,
            options: n.options ?? {},
            slots: [],
            outgoing: [],
            isSink: !(chain.edges ?? []).some((e) => e.from === n.id),
            session: null,
            finished: false,
            emitted: 0,
            retained: [],
            preview: [],
        };
    });
    const edgeSlot = new Map();
    const declared = (chain.inputs ?? []).length > 0;
    for (const node of nodes) {
        const wired = (chain.edges ?? []).some((e) => e.to === node.id);
        for (const port of node.tool.inputs) {
            let valueIndex = 0;
            (chain.edges ?? []).forEach((edge, eIdx) => {
                if (edge.to !== node.id || edgePort(edge, node.tool).name !== port.name) return;
                edgeSlot.set(eIdx, [idx.get(node.id), node.slots.length]);
                node.slots.push({
                    port: port.name,
                    index: valueIndex++,
                    meta: { type: "bytes" },
                    buffer: [],
                    ended: false,
                });
            });
            // Declared-input bindings, in input-declaration order (defines
            // value order on multi ports).
            for (const input of chain.inputs ?? []) {
                for (const bind of input.binds ?? []) {
                    if (bind.node !== node.id || bindPort(bind, node.tool).name !== port.name) continue;
                    node.slots.push({
                        port: port.name,
                        index: valueIndex++,
                        meta: { type: "bytes" },
                        buffer: [],
                        ended: false,
                        input: input.name,
                    });
                }
            }
            if (valueIndex === 0) {
                if (port.entropy) {
                    node.slots.push({
                        port: port.name,
                        index: 0,
                        meta: { type: "bytes" },
                        buffer: [],
                        ended: false,
                        entropy: true,
                    });
                } else if (!wired && !declared) {
                    // Entry node of a chain without declared inputs: the
                    // implicit input ("") feeds every port.
                    node.slots.push({
                        port: port.name,
                        index: 0,
                        meta: { type: "bytes" },
                        buffer: [],
                        ended: false,
                        input: "",
                    });
                }
            }
        }
    }
    (chain.edges ?? []).forEach((edge, eIdx) => {
        nodes[idx.get(edge.from)].outgoing.push(edgeSlot.get(eIdx));
    });
    return nodes;
}

/**
 * Execute a chain over named chunk sources: an array of
 * {name, meta, chunks} where chunks is an async iterable of Uint8Array
 * (name "" = the implicit input of a chain without declared inputs).
 * Returns Map<nodeId, result>:
 * - reservoir nodes and sinks: {ok: true, value}
 * - streaming intermediates (unless retain): {ok: true, streamed: {total, preview}}
 * - failures: {ok: false, error} (downstream nodes are absent)
 *
 * With `onSink(nodeId, chunk)`, sink output is delivered to the callback
 * as it is produced instead of being retained — sinks then report
 * {ok: true, streamed} like other streaming nodes, and memory stays
 * bounded no matter how large the output.
 */
export async function executeChainStreaming(chain, catalog, sources, retain = false, onSink = null) {
    validateChain(chain, catalog);
    const expected = (chain.inputs ?? []).length ? chain.inputs.map((i) => i.name) : [""];
    for (const src of sources) {
        if (!expected.includes(src.name)) throw new Error(`chain has no input named "${src.name}"`);
    }
    const nodes = buildEngine(chain, catalog);
    const results = new Map();

    // Static metas: source meta for input-fed slots, declared output type
    // for slots fed by streaming sources; reservoir sources overwrite at
    // delivery time (e.g. the actual image format).
    nodes.forEach((node) => {
        node.slots.forEach((slot) => {
            if (slot.input !== undefined) {
                const src = sources.find((s) => s.name === slot.input);
                if (src) slot.meta = { ...src.meta };
            }
        });
        if (node.tool.streaming) {
            for (const [t, s] of node.outgoing) {
                nodes[t].slots[s].meta = { type: node.tool.output };
            }
        }
    });

    // Open sessions for streaming nodes.
    for (const node of nodes) {
        if (node.tool.streaming) {
            node.session = await openToolStream(node.tool.module, node.tool.name, node.options);
        }
    }

    const failed = new Set();
    const queue = [];

    const emit = async (nIdx, bytes) => {
        const node = nodes[nIdx];
        if (!bytes.length) return;
        node.emitted += bytes.length;
        if (node.isSink && onSink) {
            // Awaited: a sink that can't keep up (slow download disk)
            // pauses the pump, and with it the input file read.
            await onSink(node.id, bytes);
            if (node.preview.reduce((n, c) => n + c.length, 0) < PREVIEW_CAP) {
                node.preview.push(bytes.slice(0, PREVIEW_CAP));
            }
        } else if (retain || node.isSink) {
            node.retained.push(bytes);
        } else if (node.preview.reduce((n, c) => n + c.length, 0) < PREVIEW_CAP) {
            node.preview.push(bytes.slice(0, PREVIEW_CAP));
        }
        for (const [t, s] of node.outgoing) queue.push(["chunk", t, s, bytes]);
    };

    const markFailed = (nIdx, message) => {
        results.set(nodes[nIdx].id, { ok: false, error: message });
        failed.add(nIdx);
        nodes[nIdx].session?.abort();
        nodes[nIdx].finished = true;
    };

    const finishNode = (nIdx) => {
        nodes[nIdx].finished = true;
        for (const [t, s] of nodes[nIdx].outgoing) queue.push(["end", t, s]);
    };

    const runReservoir = async (nIdx) => {
        const node = nodes[nIdx];
        const inputs = {};
        for (const slot of node.slots) {
            const value = { type: slot.meta.type, bytes: concat(slot.buffer) };
            if (slot.meta.format) value.format = slot.meta.format;
            slot.buffer = [];
            (inputs[slot.port] ??= []).push(value);
        }
        let output;
        try {
            output = await runTool(node.tool.module, node.tool.name, node.options, inputs);
        } catch (e) {
            markFailed(nIdx, e.message);
            return;
        }
        results.set(node.id, { ok: true, value: output });
        if (node.isSink && onSink) await onSink(node.id, output.bytes);
        node.emitted = output.bytes.length;
        for (const [t, s] of node.outgoing) {
            nodes[t].slots[s].meta = { type: output.type, ...(output.format ? { format: output.format } : {}) };
            queue.push(["chunk", t, s, output.bytes]);
        }
        if (node.isSink) node.retained = [];
        finishNode(nIdx);
    };

    const process = async () => {
        while (queue.length) {
            const [kind, nIdx, sIdx, bytes] = queue.shift();
            const node = nodes[nIdx];
            if (failed.has(nIdx)) continue;
            const slot = node.slots[sIdx];
            if (kind === "chunk") {
                if (node.session) {
                    try {
                        await emit(nIdx, node.session.update(slot.port, slot.index, bytes));
                    } catch (e) {
                        markFailed(nIdx, e.message);
                    }
                } else {
                    slot.buffer.push(bytes);
                }
            } else {
                slot.ended = true;
                const allEnded = node.slots.every((s) => s.ended);
                if (node.session) {
                    try {
                        await emit(nIdx, node.session.endInput(slot.port, slot.index));
                        if (allEnded && !node.finished) {
                            await emit(nIdx, node.session.finish());
                            finishNode(nIdx);
                        }
                    } catch (e) {
                        markFailed(nIdx, e.message);
                    }
                } else if (allEnded && !node.finished) {
                    await runReservoir(nIdx);
                }
            }
        }
    };

    // Driver-filled randomness first: one chunk from the browser CSPRNG,
    // visible in the ABI request like any other input.
    nodes.forEach((node, nIdx) =>
        node.slots.forEach((slot, sIdx) => {
            if (!slot.entropy) return;
            queue.push(["chunk", nIdx, sIdx, crypto.getRandomValues(new Uint8Array(ENTROPY_LEN))]);
            queue.push(["end", nIdx, sIdx]);
        }),
    );
    await process();

    // Feed each input in declaration order (mirrors core).
    for (const name of expected) {
        const src = sources.find((s) => s.name === name);
        if (!src) throw new Error(`missing chain input "${name}"`);
        const slots = [];
        nodes.forEach((node, nIdx) =>
            node.slots.forEach((slot, sIdx) => {
                if (slot.input === name) slots.push([nIdx, sIdx]);
            }),
        );
        for await (const chunk of src.chunks) {
            for (const [n, s] of slots) queue.push(["chunk", n, s, chunk]);
            await process();
        }
        for (const [n, s] of slots) queue.push(["end", n, s]);
        await process();
    }

    // Assemble results for streaming nodes.
    for (const node of nodes) {
        if (!node.session || results.has(node.id)) continue;
        if (!node.finished) continue; // upstream failed; leave absent
        if (!(node.isSink && onSink) && (retain || node.isSink)) {
            const value = { type: node.tool.output, bytes: concat(node.retained) };
            results.set(node.id, { ok: true, value });
        } else {
            const previewBytes = concat(node.preview).slice(0, PREVIEW_CAP);
            results.set(node.id, {
                ok: true,
                streamed: {
                    total: node.emitted,
                    preview: { type: node.tool.output, bytes: previewBytes },
                    truncated: node.emitted > previewBytes.length,
                },
            });
        }
    }
    return results;
}

/**
 * Execute a chain on complete in-memory values — the same push engine fed
 * single chunks, with every node's output retained (per-node previews).
 * `inputs` is one value ({type, bytes, format?}) for single-input chains,
 * or an object of name -> value for chains with declared inputs.
 */
export async function executeChain(chain, catalog, inputs) {
    const byName = inputs.bytes !== undefined ? { "": inputs } : inputs;
    const declared = (chain.inputs ?? []).length ? chain.inputs.map((i) => i.name) : [""];
    const sources = Object.entries(byName).map(([name, value]) => {
        // A single unnamed value feeds a chain whose one input is named.
        const resolved = name === "" && declared.length === 1 ? declared[0] : name;
        async function* once() {
            yield value.bytes;
        }
        return {
            name: resolved,
            meta: { type: value.type, ...(value.format ? { format: value.format } : {}) },
            chunks: once(),
        };
    });
    return executeChainStreaming(chain, catalog, sources, true);
}
