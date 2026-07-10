<script>
    import OptionsForm from "../components/OptionsForm.svelte";
    import ValueInput from "../components/ValueInput.svelte";
    import ValueOutput from "../components/ValueOutput.svelte";
    import {
        chainFromHash,
        chainToHash,
        ensureBytes,
        fileChunks,
        prettySize,
        typeCompatible,
    } from "../lib/catalog.js";
    import {
        applyParams,
        edgePort,
        executeChain,
        executeChainStreaming,
        validateChain,
    } from "../lib/dagRun.js";

    let { catalog, shared = "" } = $props();

    const NODE_W = 170;
    const NODE_H = 54;

    let nodes = $state([]); // {id, tool, options, x, y}
    let edges = $state([]); // {from, to, to_port?}
    let params = $state([]); // declared chain params (from loaded chains)
    let paramValues = $state({});
    let selectedId = $state(null);
    let pendingFrom = $state(null); // output port a connection starts from
    let paletteTool = $state("");
    let input = $state(null);
    let results = $state(new Map());
    let notice = $state(null); // {kind: "err"|"ok", text}
    let running = $state(false);
    let counter = 1;

    let selected = $derived(nodes.find((n) => n.id === selectedId) ?? null);
    let allTools = $derived([...catalog.tools.values()]);
    let entryTool = $derived.by(() => {
        const withIncoming = new Set(edges.map((e) => e.to));
        const entry = nodes.find((n) => !withIncoming.has(n.id));
        return entry ? catalog.tools.get(entry.tool) : null;
    });

    // Load a shared chain (from a share URL or the chain library).
    $effect(() => {
        if (!shared) return;
        try {
            const chain = chainFromHash(shared);
            // Careful: this effect must not read `nodes`/`edges` (which it
            // writes) or it re-triggers itself.
            nodes = layout(chain);
            edges = chain.edges ?? [];
            params = chain.params ?? [];
            paramValues = {};
            counter = chain.nodes.length + 1;
            selectedId = null;
            results = new Map();
        } catch {
            flash("err", "Could not read the shared chain from the URL.");
        }
    });

    /** Assign positions column-by-column along the DAG's depth. */
    function layout(chain) {
        const depth = new Map();
        const nodeDepth = (id) => {
            if (depth.has(id)) return depth.get(id);
            const parents = (chain.edges ?? []).filter((e) => e.to === id);
            const d = parents.length ? 1 + Math.max(...parents.map((e) => nodeDepth(e.from))) : 0;
            depth.set(id, d);
            return d;
        };
        const perColumn = new Map();
        return chain.nodes.map((n) => {
            const d = nodeDepth(n.id);
            const row = perColumn.get(d) ?? 0;
            perColumn.set(d, row + 1);
            return { ...n, options: n.options ?? {}, x: 40 + d * 230, y: 40 + row * 100 };
        });
    }

    function chainData() {
        return {
            version: 1,
            params: $state.snapshot(params),
            nodes: nodes.map(({ id, tool, options }) => ({
                id,
                tool,
                options: $state.snapshot(options),
            })),
            edges: $state.snapshot(edges),
        };
    }

    function flash(kind, text) {
        notice = { kind, text };
        setTimeout(() => (notice = null), 3500);
    }

    function addNode() {
        if (!paletteTool) return;
        const id = `n${counter++}`;
        nodes.push({
            id,
            tool: paletteTool,
            options: {},
            x: 40 + ((nodes.length * 40) % 400),
            y: 40 + ((nodes.length * 70) % 280),
        });
        selectedId = id;
    }

    function removeNode(id) {
        nodes = nodes.filter((n) => n.id !== id);
        edges = edges.filter((e) => e.from !== id && e.to !== id);
        if (selectedId === id) selectedId = null;
        results = new Map();
    }

    function tryConnect(from, to, portName) {
        pendingFrom = null;
        if (from === to) return;
        const toTool = catalog.tools.get(nodes.find((n) => n.id === to).tool);
        const fromTool = catalog.tools.get(nodes.find((n) => n.id === from).tool);
        const port = toTool.inputs.find((p) => p.name === portName);
        if (!typeCompatible(catalog.coercions, fromTool.output, port.type)) {
            flash(
                "err",
                `${fromTool.label} outputs ${fromTool.output}; port “${port.name}” needs ${port.type}.`,
            );
            return;
        }
        const edge = { from, to };
        if (toTool.inputs.length > 1) edge.to_port = portName;
        const candidate = [...$state.snapshot(edges), edge];
        try {
            validateChain({ version: 1, nodes, edges: candidate }, catalog);
        } catch (e) {
            flash("err", e.message);
            return;
        }
        edges = candidate;
        results = new Map();
    }

    async function run() {
        if (!input) {
            flash("err", "Provide an input first.");
            return;
        }
        let effective;
        try {
            effective = applyParams(chainData(), $state.snapshot(paramValues));
            validateChain(effective, catalog);
        } catch (e) {
            flash("err", e.message);
            return;
        }
        running = true;
        try {
            if (input.file && !input.bytes) {
                // Large file: push it through chunk-by-chunk. Streaming
                // intermediates keep a capped preview instead of the value.
                results = await executeChainStreaming(
                    effective,
                    catalog,
                    fileChunks(input.file),
                    { type: input.type },
                    false,
                );
            } else {
                results = await executeChain(effective, catalog, await ensureBytes(input));
            }
            const failed = [...results.values()].filter((r) => !r.ok).length;
            flash(failed ? "err" : "ok", failed ? `${failed} step(s) failed` : "Chain ran ✓");
        } finally {
            running = false;
        }
    }

    async function share() {
        if (!nodes.length) return;
        const hash = chainToHash(chainData());
        location.hash = `#/builder/${hash}`;
        await navigator.clipboard.writeText(location.href);
        flash("ok", "Share link copied — it contains the chain definition, never your data.");
    }

    // ---- geometry ----

    /** Ports rendered in the UI (entropy ports are driver-filled). */
    function ports(tool) {
        return tool.inputs.filter((p) => !p.entropy);
    }

    function portY(tool, index) {
        return (NODE_H * (index + 1)) / (ports(tool).length + 1);
    }

    function edgePath(edge) {
        const from = nodes.find((n) => n.id === edge.from);
        const to = nodes.find((n) => n.id === edge.to);
        const toTool = catalog.tools.get(to.tool);
        const port = edgePort(edge, toTool);
        const portIndex = ports(toTool).findIndex((p) => p.name === port.name);
        const x1 = from.x + NODE_W;
        const y1 = from.y + NODE_H / 2;
        const x2 = to.x;
        const y2 = to.y + portY(toTool, portIndex);
        const bend = Math.max(40, (x2 - x1) / 2);
        return `M ${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}`;
    }

    function nodeStroke(node) {
        const r = results.get(node.id);
        if (r) return r.ok ? "var(--ok)" : "var(--err)";
        return selectedId === node.id ? "var(--accent)" : "var(--border)";
    }

    /** Order badge for edges into a multi port: {n, x, y} or null. */
    function edgeBadge(edge) {
        const to = nodes.find((n) => n.id === edge.to);
        const toTool = catalog.tools.get(to.tool);
        const port = edgePort(edge, toTool);
        if (!port.multi) return null;
        const siblings = edges.filter(
            (e) => e.to === edge.to && edgePort(e, toTool).name === port.name,
        );
        if (siblings.length < 2) return null;
        const portIndex = ports(toTool).findIndex((p) => p.name === port.name);
        return {
            n: siblings.indexOf(edge) + 1,
            x: to.x - 20,
            y: to.y + portY(toTool, portIndex),
        };
    }

    // ---- dragging ----
    let drag = null; // {id, dx, dy}
    let svgEl;

    function svgPoint(event) {
        const rect = svgEl.getBoundingClientRect();
        return { x: event.clientX - rect.left, y: event.clientY - rect.top };
    }

    function startDrag(node, event) {
        const p = svgPoint(event);
        drag = { id: node.id, dx: p.x - node.x, dy: p.y - node.y };
        selectedId = node.id;
    }

    function onMove(event) {
        if (!drag) return;
        const node = nodes.find((n) => n.id === drag.id);
        const p = svgPoint(event);
        node.x = Math.max(0, p.x - drag.dx);
        node.y = Math.max(0, p.y - drag.dy);
    }
</script>

<h1>Chain builder</h1>
<p class="dim">
    Compose tools into a pipeline: add nodes, then click a node's <em>output dot</em> followed by
    another node's <em>input dot</em> to connect them. Connections are type-checked. Click an edge
    to remove it.
</p>

<div class="toolbar">
    <select bind:value={paletteTool}>
        <option value="" disabled>Add a tool…</option>
        {#each allTools as tool (tool.name)}
            <option value={tool.name}>
                {tool.label} ({tool.inputs.map((p) => p.type + (p.multi ? "…" : "")).join(" + ")} → {tool.output})
            </option>
        {/each}
    </select>
    <button class="btn secondary" onclick={addNode} disabled={!paletteTool}>Add node</button>
    <span class="spacer"></span>
    <button class="btn" onclick={run} disabled={running || !nodes.length}>
        {running ? "Running…" : "▶ Run chain"}
    </button>
    <button class="btn secondary" onclick={share} disabled={!nodes.length}>Share</button>
</div>

{#if notice}
    <p class={notice.kind === "err" ? "error" : "ok-note"}>{notice.text}</p>
{/if}

<div class="layout">
    <div class="left">
        {#if params.length}
            <section class="card">
                <h2>Chain settings</h2>
                <OptionsForm specs={params} bind:values={paramValues} />
            </section>
        {/if}

        <section class="card input-panel">
            <h2>
                Chain input {#if entryTool}<span class="mono dim"
                        >({entryTool.inputs.map((p) => p.type + (p.multi ? "…" : "")).join(" + ")})</span
                    >{/if}
            </h2>
            <ValueInput bind:value={input} hint={entryTool?.inputs[0]?.type ?? "text"} />
        </section>

        <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
        <svg
            bind:this={svgEl}
            class="canvas"
            onpointermove={onMove}
            onpointerup={() => (drag = null)}
            onclick={(e) => {
                if (e.target === svgEl) {
                    selectedId = null;
                    pendingFrom = null;
                }
            }}
        >
            {#each edges as edge, i (i)}
                {@const badge = edgeBadge(edge)}
                <path
                    class="edge"
                    d={edgePath(edge)}
                    onclick={() => {
                        edges = edges.filter((e) => e !== edge);
                        results = new Map();
                    }}
                >
                    <title>click to remove</title>
                </path>
                {#if badge}
                    <g class="badge" transform="translate({badge.x}, {badge.y})">
                        <circle r="8" />
                        <text y="3.5">{badge.n}</text>
                    </g>
                {/if}
            {/each}

            {#each nodes as node (node.id)}
                {@const tool = catalog.tools.get(node.tool)}
                <g transform="translate({node.x}, {node.y})">
                    <rect
                        width={NODE_W}
                        height={NODE_H}
                        rx="8"
                        stroke={nodeStroke(node)}
                        onpointerdown={(e) => startDrag(node, e)}
                    />
                    <text class="title" x="12" y="22">{tool.label}</text>
                    <text class="sub" x="12" y="40">
                        {tool.inputs.map((p) => p.type + (p.multi ? "…" : "")).join("+")} → {tool.output}
                    </text>
                    {#each ports(tool) as port, i (port.name)}
                        <circle
                            class="port"
                            class:target={pendingFrom && pendingFrom !== node.id}
                            cx="0"
                            cy={portY(tool, i)}
                            r="7"
                            onclick={(e) => {
                                e.stopPropagation();
                                if (pendingFrom) tryConnect(pendingFrom, node.id, port.name);
                            }}
                        >
                            <title>{port.name} ({port.type}{port.multi ? ", accepts many" : ""})</title>
                        </circle>
                        {#if ports(tool).length > 1 || port.multi}
                            <text class="port-label" x="-11" y={portY(tool, i) + 3}>
                                {port.name}{port.multi ? "…" : ""}
                            </text>
                        {/if}
                    {/each}
                    <!-- output port -->
                    <circle
                        class="port out"
                        class:active={pendingFrom === node.id}
                        cx={NODE_W}
                        cy={NODE_H / 2}
                        r="7"
                        onclick={(e) => {
                            e.stopPropagation();
                            pendingFrom = pendingFrom === node.id ? null : node.id;
                        }}
                    />
                </g>
            {/each}

            {#if !nodes.length}
                <text class="empty" x="50%" y="50%">Add a tool above to start building.</text>
            {/if}
        </svg>
    </div>

    <aside class="card side">
        {#if selected}
            {@const tool = catalog.tools.get(selected.tool)}
            {@const result = results.get(selected.id)}
            <h2>{tool.label}</h2>
            <p class="dim">{tool.description}</p>
            {#if tool.options.length}
                <OptionsForm specs={tool.options} bind:values={selected.options} />
            {/if}
            <button class="btn danger" onclick={() => removeNode(selected.id)}>Remove node</button>
            {#if result}
                <h2>Step output</h2>
                {#if result.ok && result.value}
                    <ValueOutput value={result.value} />
                {:else if result.ok && result.streamed}
                    <p class="dim">
                        Streamed {prettySize(result.streamed.total)} through this
                        step{result.streamed.truncated ? " — preview:" : ":"}
                    </p>
                    <ValueOutput value={result.streamed.preview} />
                {:else if !result.ok}
                    <p class="error">{result.error}</p>
                {/if}
            {/if}
        {:else}
            <p class="dim">
                Select a node to edit its options; after a run, its output appears here too.
            </p>
            <p class="dim">
                Or start from a ready-made chain in the <a href="#/chains">library</a>.
            </p>
        {/if}
    </aside>
</div>

<style>
    .toolbar {
        display: flex;
        gap: 0.6rem;
        align-items: center;
        margin-bottom: 0.8rem;
    }
    .toolbar select {
        width: auto;
        max-width: 20rem;
    }
    .spacer {
        flex: 1;
    }
    .ok-note {
        color: var(--ok);
    }
    .layout {
        display: grid;
        grid-template-columns: 1fr;
        gap: 1rem;
        align-items: start;
    }
    @media (min-width: 900px) {
        .layout {
            grid-template-columns: 1fr 300px;
        }
    }
    .left {
        display: flex;
        flex-direction: column;
        gap: 1rem;
        min-width: 0;
    }
    .left h2 {
        margin-top: 0;
    }
    .canvas {
        width: 100%;
        height: 460px;
        background:
            linear-gradient(var(--bg-raised) 1px, transparent 1px) 0 0 / 100% 24px,
            linear-gradient(90deg, var(--bg-raised) 1px, transparent 1px) 0 0 / 24px 100%,
            var(--bg-input);
        border: 1px solid var(--border);
        border-radius: var(--radius);
        touch-action: none;
    }
    rect {
        fill: var(--bg-raised);
        stroke-width: 1.5;
        cursor: grab;
    }
    .title {
        fill: var(--text);
        font-size: 13px;
        font-weight: 600;
        pointer-events: none;
    }
    .sub {
        fill: var(--text-dim);
        font-size: 11px;
        font-family: ui-monospace, monospace;
        pointer-events: none;
    }
    .port {
        fill: var(--bg-input);
        stroke: var(--text-dim);
        stroke-width: 1.5;
        cursor: pointer;
    }
    .port:hover,
    .port.target {
        stroke: var(--accent);
        fill: var(--accent-dim);
    }
    .port.out.active {
        fill: var(--accent);
        stroke: var(--accent);
    }
    .port-label {
        fill: var(--text-dim);
        font-size: 10px;
        font-family: ui-monospace, monospace;
        text-anchor: end;
        pointer-events: none;
    }
    .badge circle {
        fill: var(--accent-dim);
    }
    .badge text {
        fill: #04121e;
        font-size: 10px;
        font-weight: 700;
        text-anchor: middle;
        pointer-events: none;
    }
    .edge {
        fill: none;
        stroke: var(--accent-dim);
        stroke-width: 2;
        cursor: pointer;
    }
    .edge:hover {
        stroke: var(--err);
    }
    .empty {
        fill: var(--text-dim);
        font-size: 14px;
        text-anchor: middle;
    }
    .side h2 {
        margin-top: 0;
    }
</style>
