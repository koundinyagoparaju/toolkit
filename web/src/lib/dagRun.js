// Client-side chain execution: walk the DAG in topological order, calling
// each node's pack. Structure/type validation mirrors the Rust executor
// (crates/core/src/chain.rs); the type rules come from the generated
// coercion matrix, so the two sides cannot drift apart.

import { typeCompatible } from "./catalog.js";
import { runTool } from "./wasm.js";

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

    // A node with any incoming edge must have every port wired.
    for (const node of chain.nodes) {
        if (!incoming.get(node.id)) continue;
        const tool = catalog.tools.get(node.tool);
        const missing = tool.inputs.filter((p) => !wired.has(`${node.id} ${p.name}`));
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

/**
 * Execute a chain. Returns per-node results:
 * Map<nodeId, {ok: true, value} | {ok: false, error}>.
 * Downstream nodes of a failed node are skipped (absent from the map).
 */
export async function executeChain(chain, catalog, input) {
    const order = validateChain(chain, catalog);
    const results = new Map();
    const hasIncoming = new Set((chain.edges ?? []).map((e) => e.to));

    for (const id of order) {
        const node = chain.nodes.find((n) => n.id === id);
        const tool = catalog.tools.get(node.tool);

        // Pull inputs from predecessor results, scanning edges in
        // declaration order — which defines value order on multi ports
        // (mirrors the Rust executor).
        const inputs = {};
        let ready = true;
        for (const port of tool.inputs) {
            let values;
            if (hasIncoming.has(id)) {
                values = [];
                for (const edge of (chain.edges ?? []).filter((e) => e.to === id)) {
                    if (edgePort(edge, tool).name !== port.name) continue;
                    const upstream = results.get(edge.from);
                    if (!upstream?.ok) {
                        ready = false; // upstream failed or was skipped
                        break;
                    }
                    values.push(upstream.value);
                }
            } else {
                values = [input];
            }
            if (!ready) break;
            inputs[port.name] = values;
        }
        if (!ready) continue;

        try {
            const value = await runTool(tool.module, node.tool, node.options ?? {}, inputs);
            results.set(id, { ok: true, value });
        } catch (error) {
            results.set(id, { ok: false, error: error.message });
        }
    }
    return results;
}
