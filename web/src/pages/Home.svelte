<script>
    import CliCommand from "../components/CliCommand.svelte";
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

<section class="card cli-callout">
    <h2>Also lives in your terminal</h2>
    <p class="dim">
        Every tool and chain above, as one static binary — pipe-friendly, offline, streaming
        (gigabytes in a few MB of memory), and just as private: it contains no network code at
        all.
    </p>
    <CliCommand
        command="curl -fsSL https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main/scripts/install.sh | sh"
    />
    <pre class="dim">echo -n $JWT | toolkit chain 'jwt-decode | json-format'
toolkit chain -n image-web-ready --set width=800 -i photo.png -o photo.jpg
toolkit chain -n file-checksum -i backup.iso</pre>
    <p class="dim">
        Piping a script into your shell means trusting it — so
        <a
            href="https://github.com/koundinyagoparaju/toolkit/blob/main/scripts/install.sh"
            target="_blank"
            rel="noreferrer">read it first</a
        >
        (~90 lines, checksum-verified downloads), or
        <a
            href="https://github.com/koundinyagoparaju/toolkit#cli"
            target="_blank"
            rel="noreferrer">build from source</a
        >.
    </p>
</section>

<style>
    .cli-callout {
        margin-top: 2rem;
        max-width: 46rem;
    }
    .cli-callout h2 {
        margin-top: 0;
    }
    .cli-callout pre {
        margin: 0.8rem 0 0.4rem;
        font-size: 0.82rem;
        overflow-x: auto;
    }
    .hero {
        margin: 1.5rem 0 2rem;
        max-width: 42rem;
    }
    .hero input {
        margin-top: 0.8rem;
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
