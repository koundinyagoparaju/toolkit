<script>
    import CliCommand from "../components/CliCommand.svelte";
    import OptionsForm from "../components/OptionsForm.svelte";
    import ValueInput from "../components/ValueInput.svelte";
    import ValueOutput from "../components/ValueOutput.svelte";
    import { concatBytes, ensureBytes, prettySize, textValue } from "../lib/catalog.js";
    import { openToolStream, runTool } from "../lib/wasm.js";

    let { catalog, name } = $props();

    let tool = $derived(catalog.tools.get(name));
    // Entropy ports are auto-filled from the browser CSPRNG, never shown.
    let visiblePorts = $derived(tool ? tool.inputs.filter((p) => !p.entropy) : []);
    // port name -> ordered value slots (single ports have exactly one slot;
    // multi ports grow/shrink with Add/Remove).
    let inputs = $state({});
    let options = $state({});
    let output = $state(null);
    let error = $state(null);
    let running = $state(false);

    let ready = $derived(
        tool &&
            visiblePorts.every(
                (p) => (inputs[p.name] ?? []).length > 0 && inputs[p.name].every(Boolean),
            ),
    );

    // Reset state when navigating between tools.
    $effect(() => {
        void name;
        inputs = tool ? Object.fromEntries(visiblePorts.map((p) => [p.name, [null]])) : {};
        options = {};
        output = null;
        error = null;
    });

    // Auto-run whenever the inputs or options change (debounced).
    // Deliberately read every value slot and option entry: editing an
    // existing input *replaces* inputs[port][i] (a deep write), which a
    // bare `void inputs` would not track — the effect would only fire on
    // the first empty→filled transition and the output would go stale.
    $effect(() => {
        for (const port of visiblePorts) for (const v of inputs[port.name] ?? []) void v;
        for (const key of Object.keys(options)) void options[key];
        void name;
        if (!ready) {
            output = null;
            error = null;
            return;
        }
        const timer = setTimeout(run, 250);
        return () => clearTimeout(timer);
    });

    let progress = $state(0);

    /** Sample inputs, straight from the manifest — present only when the
     *  tool declares an example for every visible port (enforced runnable
     *  by a CLI test, so the button can never demo an error). */
    let example = $derived(
        tool && visiblePorts.length && visiblePorts.every((p) => p.example)
            ? Object.fromEntries(visiblePorts.map((p) => [p.name, p.example]))
            : null,
    );

    function tryExample() {
        for (const port of visiblePorts) {
            inputs[port.name] = [{ ...textValue(example[port.name]), example: true }];
        }
    }

    /** Up to three tools that share keywords with this one — the "you
     *  might actually be after…" links. Same-pack tools get +1, so one
     *  shared keyword suffices within a family (image-resize ⇄
     *  image-crop) while cross-pack links still need two. */
    let related = $derived.by(() => {
        if (!tool) return [];
        const mine = new Set(tool.keywords);
        return [...catalog.tools.values()]
            .filter((t) => t.name !== tool.name)
            .map((t) => [
                t.keywords.filter((k) => mine.has(k)).length +
                    (t.pack === tool.pack && t.keywords.some((k) => mine.has(k)) ? 1 : 0),
                t,
            ])
            .filter(([score]) => score >= 2)
            .sort((a, b) => b[0] - a[0])
            .slice(0, 3)
            .map(([, t]) => t);
    });

    /** Terminal equivalents of the current tool + option values, most
     *  idiomatic first: stdin pipe, input as an argument, file input. */
    let cliCommands = $derived.by(() => {
        const quote = (v) => (/^[A-Za-z0-9._-]+$/.test(String(v)) ? String(v) : `'${String(v).replaceAll("'", `'\\''`)}'`);
        const sets = Object.entries(options)
            .filter(([, v]) => v !== undefined && v !== "")
            .map(([k, v]) => ` --set ${k}=${quote(v)}`)
            .join("");
        const base = `toolkit run-tool ${tool.name}${sets}`;
        if (!visiblePorts.length) return [base];

        // The page's typed input, when it's short and single-line, makes
        // the commands concrete (same live spirit as the options).
        const typed = (() => {
            const v = inputs[visiblePorts[0].name]?.[0];
            if (!v || v.file || !v.bytes || v.type !== "text") return null;
            const text = new TextDecoder().decode(v.bytes);
            return text.length > 0 && text.length <= 40 && !text.includes("\n") ? text : null;
        })();
        const value = quote(typed ?? "<text>");

        if (visiblePorts.length === 1 && !visiblePorts[0].multi) {
            const port = visiblePorts[0];
            if (port.type === "image") {
                return [`cat <file> | ${base}`, `${base} -i <file>`];
            }
            return [`echo -n ${value} | ${base}`, `${base} ${value}`, `${base} -i <file>`];
        }
        if (visiblePorts.length === 1) {
            // one variable-arity port
            return [`${base} ${value} ${value}`, `${base} -i <file> -i <file>`];
        }
        const ins = visiblePorts
            .map((p) => (p.multi ? " -i <file> -i <file>" : ` -i ${p.name}=<file>`))
            .join("");
        // Named ports take paths, and a process substitution is a path —
        // so any command's output can feed a port without a temp file.
        const firstSingle = visiblePorts.findIndex((p) => !p.multi);
        if (firstSingle === -1) return [`${base}${ins}`];
        const subs = visiblePorts
            .map((p, i) =>
                p.multi
                    ? " -i <file> -i <file>"
                    : ` -i ${p.name}=${i === firstSingle ? "<(command)" : "<file>"}`,
            )
            .join("");
        return [`${base}${ins}`, `${base}${subs}`];
    });

    async function run() {
        if (!ready) return;
        running = true;
        progress = 0;
        try {
            output = tool.streaming
                ? await runStreaming()
                : await runBuffered();
            error = null;
        } catch (e) {
            output = null;
            error = e.message;
        } finally {
            running = false;
        }
    }

    async function runBuffered() {
        const payload = {};
        for (const p of tool.inputs) {
            payload[p.name] = p.entropy
                ? [{ type: "bytes", bytes: crypto.getRandomValues(new Uint8Array(1024)) }]
                : await Promise.all($state.snapshot(inputs[p.name]).map(ensureBytes));
        }
        return runTool(tool.module, tool.name, options, payload);
    }

    /** Sources are consumed sequentially in port order; a File is fed
     *  chunk-by-chunk via file.stream() and never fully loaded. */
    async function runStreaming() {
        const session = await openToolStream(
            tool.module,
            tool.name,
            $state.snapshot(options),
        );
        try {
            const out = [];
            for (const port of tool.inputs) {
                const values = $state.snapshot(inputs[port.name]);
                for (let i = 0; i < values.length; i++) {
                    const v = values[i];
                    if (v.file) {
                        const reader = v.file.stream().getReader();
                        for (;;) {
                            const { done, value } = await reader.read();
                            if (done) break;
                            out.push(session.update(port.name, i, value));
                            progress += value.length;
                        }
                    } else {
                        out.push(session.update(port.name, i, v.bytes));
                        progress += v.bytes.length;
                    }
                    out.push(session.endInput(port.name, i));
                }
            }
            out.push(session.finish());
            return { type: tool.output, bytes: concatBytes(out) };
        } catch (e) {
            session.abort();
            throw e;
        }
    }

    function addSlot(port) {
        inputs[port] = [...inputs[port], null];
    }

    function removeSlot(port, index) {
        inputs[port] = inputs[port].filter((_, i) => i !== index);
    }
</script>

{#if !tool}
    <p class="error">No tool named “{name}”. <a href="#/">Back to the catalog.</a></p>
{:else}
    <h1>{tool.label}</h1>
    <p class="dim">{tool.description}</p>

    <div class="layout">
        <div class="main">
            {#each visiblePorts as port (port.name)}
                <section>
                    <h2>
                        {visiblePorts.length > 1 ? `Input “${port.name}”` : "Input"}
                        <span class="mono dim">({port.type}{port.multi ? "…" : ""})</span>
                    </h2>
                    {#if port.description}
                        <p class="dim port-desc">{port.description}</p>
                    {/if}
                    {#each inputs[port.name] ?? [] as _, i (i)}
                        <div class="slot">
                            <ValueInput bind:value={inputs[port.name][i]} hint={port.type} />
                            {#if port.multi && (inputs[port.name]?.length ?? 0) > 1}
                                <button
                                    class="btn danger remove"
                                    onclick={() => removeSlot(port.name, i)}
                                >
                                    Remove
                                </button>
                            {/if}
                        </div>
                    {/each}
                    {#if port.multi}
                        <button class="btn secondary" onclick={() => addSlot(port.name)}>
                            + Add another
                        </button>
                    {/if}
                </section>
            {/each}
            <section>
                <h2>
                    Output <span class="mono dim">({tool.output})</span>
                    {#if running}<span class="dim">
                            · running…{#if tool.streaming && progress}
                                {prettySize(progress)} processed{/if}</span
                        >{/if}
                </h2>
                {#if error}
                    <p class="error">{error}</p>
                {:else if output}
                    <ValueOutput value={output} />
                    {#if tool.inputs.some((p) => p.entropy)}
                        <p><button class="btn secondary" onclick={run}>Generate again</button></p>
                    {/if}
                {:else if visiblePorts.length}
                    <p class="dim">
                        Provide {visiblePorts.length > 1 ? "all inputs" : "input"} above — the tool
                        runs automatically.
                        {#if example}
                            <button class="btn secondary example" onclick={tryExample}>
                                Try an example
                            </button>
                        {/if}
                    </p>
                {:else}
                    <p class="dim">Generating…</p>
                {/if}
            </section>

            <section class="cli-hint">
                <h2>Same tool, in your terminal</h2>
                <div class="variants">
                    {#each cliCommands as command (command)}
                        <CliCommand {command} />
                    {/each}
                </div>
                <p class="dim">
                    The CLI is one static binary with every tool and chain — pipe-friendly,
                    offline, and it streams (gigabytes in a few MB of memory).
                    <a href="#/cli">Install it</a>.
                </p>
            </section>

            {#if related.length}
                <p class="related dim">
                    Related tools:
                    {#each related as r, i (r.name)}{#if i}&nbsp;·
                        {/if}<a href="#/tool/{r.name}">{r.label}</a>{/each}
                </p>
            {/if}
        </div>
        {#if tool.options.length}
            <aside class="card">
                <h2>Options</h2>
                <OptionsForm specs={tool.options} bind:values={options} />
            </aside>
        {/if}
    </div>
{/if}

<style>
    .cli-hint .variants {
        display: flex;
        flex-direction: column;
        gap: 0.4rem;
    }
    .cli-hint p {
        margin-top: 0.5rem;
        font-size: 0.82rem;
    }
    .example {
        margin-left: 0.5rem;
        font-size: 0.82rem;
        padding: 0.25rem 0.7rem;
    }
    .related {
        font-size: 0.85rem;
        margin: 0;
    }
    .layout {
        display: grid;
        grid-template-columns: 1fr;
        gap: 1.2rem;
        align-items: start;
    }
    @media (min-width: 780px) {
        .layout {
            grid-template-columns: 1fr 260px;
        }
    }
    .main {
        display: flex;
        flex-direction: column;
        gap: 1.2rem;
        min-width: 0;
    }
    .slot {
        margin-bottom: 0.6rem;
    }
    .port-desc {
        margin: -0.4rem 0 0.5rem;
        font-size: 0.82rem;
    }
    .slot .remove {
        margin-top: 0.35rem;
    }
    aside h2 {
        margin-top: 0;
    }
</style>
