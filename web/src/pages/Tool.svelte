<script>
    import OptionsForm from "../components/OptionsForm.svelte";
    import ValueInput from "../components/ValueInput.svelte";
    import ValueOutput from "../components/ValueOutput.svelte";
    import { runTool } from "../lib/wasm.js";

    let { catalog, name } = $props();

    let tool = $derived(catalog.tools.get(name));
    // port name -> ordered value slots (single ports have exactly one slot;
    // multi ports grow/shrink with Add/Remove).
    let inputs = $state({});
    let options = $state({});
    let output = $state(null);
    let error = $state(null);
    let running = $state(false);

    let ready = $derived(
        tool &&
            tool.inputs.every((p) => (inputs[p.name] ?? []).length > 0 && inputs[p.name].every(Boolean)),
    );

    // Reset state when navigating between tools.
    $effect(() => {
        void name;
        inputs = tool ? Object.fromEntries(tool.inputs.map((p) => [p.name, [null]])) : {};
        options = {};
        output = null;
        error = null;
    });

    // Auto-run whenever the inputs or options change (debounced).
    $effect(() => {
        void inputs;
        void options;
        if (!ready) {
            output = null;
            error = null;
            return;
        }
        const timer = setTimeout(run, 250);
        return () => clearTimeout(timer);
    });

    async function run() {
        if (!ready) return;
        running = true;
        try {
            const payload = Object.fromEntries(
                tool.inputs.map((p) => [p.name, $state.snapshot(inputs[p.name])]),
            );
            output = await runTool(tool.module, tool.name, options, payload);
            error = null;
        } catch (e) {
            output = null;
            error = e.message;
        } finally {
            running = false;
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
            {#each tool.inputs as port (port.name)}
                <section>
                    <h2>
                        {tool.inputs.length > 1 ? `Input “${port.name}”` : "Input"}
                        <span class="mono dim">({port.type}{port.multi ? "…" : ""})</span>
                    </h2>
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
                    {#if running}<span class="dim"> · running…</span>{/if}
                </h2>
                {#if error}
                    <p class="error">{error}</p>
                {:else if output}
                    <ValueOutput value={output} />
                {:else}
                    <p class="dim">
                        Provide {tool.inputs.length > 1 ? "all inputs" : "input"} above — the tool
                        runs automatically.
                    </p>
                {/if}
            </section>
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
    .slot .remove {
        margin-top: 0.35rem;
    }
    aside h2 {
        margin-top: 0;
    }
</style>
