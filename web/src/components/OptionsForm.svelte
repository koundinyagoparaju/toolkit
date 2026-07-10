<script>
    // Renders a form for a tool's OptionSpec list. The manifest is the
    // single source of truth — adding an option to a tool in Rust makes it
    // appear here with no web changes.
    let { specs, values = $bindable() } = $props();

    function set(name, value) {
        const next = { ...values };
        if (value === "" || value === null || value === undefined) {
            delete next[name];
        } else {
            next[name] = value;
        }
        values = next;
    }

    function numeric(spec, raw) {
        if (raw === "") return set(spec.name, undefined);
        const n = Number(raw);
        if (!Number.isNaN(n)) set(spec.name, spec.kind === "integer" ? Math.trunc(n) : n);
    }
</script>

{#each specs as spec (spec.name)}
    <div class="option">
        {#if spec.kind === "bool"}
            <label class="check">
                <input
                    type="checkbox"
                    checked={values[spec.name] ?? spec.default ?? false}
                    onchange={(e) => set(spec.name, e.currentTarget.checked)}
                />
                {spec.label}
            </label>
        {:else if spec.kind === "enum"}
            <label for="opt-{spec.name}">{spec.label}{spec.required ? " *" : ""}</label>
            <select
                id="opt-{spec.name}"
                value={values[spec.name] ?? spec.default ?? ""}
                onchange={(e) => set(spec.name, e.currentTarget.value)}
            >
                {#if !spec.required && spec.default === undefined}
                    <option value=""></option>
                {/if}
                {#each spec.values as v (v)}
                    <option value={v}>{v}</option>
                {/each}
            </select>
        {:else if spec.kind === "integer" || spec.kind === "float"}
            <label for="opt-{spec.name}">{spec.label}{spec.required ? " *" : ""}</label>
            <input
                id="opt-{spec.name}"
                type="number"
                min={spec.min}
                max={spec.max}
                step={spec.kind === "integer" ? 1 : "any"}
                value={values[spec.name] ?? spec.default ?? ""}
                oninput={(e) => numeric(spec, e.currentTarget.value)}
            />
        {:else}
            <label for="opt-{spec.name}">{spec.label}{spec.required ? " *" : ""}</label>
            <input
                id="opt-{spec.name}"
                type="text"
                value={values[spec.name] ?? spec.default ?? ""}
                oninput={(e) => set(spec.name, e.currentTarget.value)}
            />
        {/if}
        {#if spec.description}
            <p class="hint">{spec.description}</p>
        {/if}
    </div>
{/each}

<style>
    .option {
        margin-bottom: 0.9rem;
    }
    .check {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        color: var(--text);
        font-size: 0.95rem;
    }
    .check input {
        width: auto;
    }
    .hint {
        margin: 0.25rem 0 0;
        font-size: 0.78rem;
        color: var(--text-dim);
    }
</style>
