<script>
    let { catalog } = $props();

    let query = $state("");

    let filtered = $derived.by(() => {
        const q = query.trim().toLowerCase();
        return catalog.packs.map((pack) => ({
            ...pack,
            tools: pack.tools.filter(
                (t) =>
                    !q ||
                    t.name.includes(q) ||
                    t.label.toLowerCase().includes(q) ||
                    t.keywords.some((k) => k.includes(q)),
            ),
        }));
    });
</script>

<header class="hero">
    <h1>The everyday tools, always at hand.</h1>
    <p class="dim">
        Decode, convert, hash, inspect, resize — the small jobs that come up all the time, in one
        fast place. And because everything runs in your browser, your data never leaves your
        device. <a href="#/trust">Verify it yourself.</a>
    </p>
    <input type="text" placeholder="Search tools… (base64, jwt, resize, …)" bind:value={query} />
    <p class="cli-strip dim">
        Prefer the terminal? All of this is also <strong>one static binary</strong> with no
        network code — <a href="#/cli">install the CLI</a>.
    </p>
</header>

{#each filtered as pack (pack.name)}
    {#if pack.tools.length}
        <section>
            <h2 class="dim">{pack.name} tools</h2>
            <div class="grid">
                {#each pack.tools as tool (tool.name)}
                    <a class="card tool" href="#/tool/{tool.name}">
                        <strong>{tool.label}</strong>
                        <span class="types mono dim"
                            >{tool.inputs.map((p) => p.type + (p.multi ? "…" : "")).join(" + ")} → {tool.output}</span
                        >
                        <span class="desc dim">{tool.description}</span>
                    </a>
                {/each}
            </div>
        </section>
    {/if}
{/each}

<style>
    .hero {
        margin: 1.5rem 0 2rem;
        max-width: 42rem;
    }
    .hero input {
        margin-top: 0.8rem;
    }
    .cli-strip {
        margin: 0.7rem 0 0;
        font-size: 0.85rem;
    }
    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
        gap: 0.8rem;
    }
    .tool {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
        color: var(--text);
    }
    .tool:hover {
        text-decoration: none;
        border-color: var(--accent-dim);
    }
    .types {
        font-size: 0.75rem;
    }
    .desc {
        font-size: 0.82rem;
    }
</style>
