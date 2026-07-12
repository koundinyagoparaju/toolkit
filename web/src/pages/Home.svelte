<script>
    let { catalog } = $props();

    let query = $state("");
    let searchBox = $state(null);

    // Friendlier section identity than the raw pack names — a new visitor
    // reads these; a regular scans the colors.
    const packMeta = {
        text: {
            title: "Text & encodings",
            blurb: "Base64, hex, URLs, JSON, JWTs, diffs — the everyday copy-paste jobs.",
            color: "var(--pack-text)",
        },
        image: {
            title: "Images",
            blurb: "Resize, convert, crop, strip metadata, QR codes — nothing is uploaded.",
            color: "var(--pack-image)",
        },
        crypto: {
            title: "Crypto & random",
            blurb: "Hashes, HMACs, UUIDs, passwords, random bytes.",
            color: "var(--pack-crypto)",
        },
        data: {
            title: "Data, time & units",
            blurb: "JSON ⇄ YAML/TOML/CSV, timestamps, colors, regex, unit conversion.",
            color: "var(--pack-data)",
        },
    };
    const meta = (pack) => packMeta[pack.name] ?? { title: `${pack.name} tools`, blurb: "", color: "var(--accent)" };

    const toolCount = catalog.packs.reduce((n, p) => n + p.tools.length, 0);

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
    let matches = $derived(filtered.reduce((n, p) => n + p.tools.length, 0));

    // "/" focuses search from anywhere on the page; Enter opens the tool
    // when the search has narrowed things down to exactly one.
    function onKeydown(e) {
        if (e.key === "/" && !/^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName)) {
            e.preventDefault();
            searchBox?.focus();
        }
    }
    function onSearchKeydown(e) {
        if (e.key === "Enter" && matches === 1) {
            const tool = filtered.flatMap((p) => p.tools)[0];
            location.hash = `#/tool/${tool.name}`;
        }
    }

    function jumpTo(name) {
        document.getElementById(`pack-${name}`)?.scrollIntoView({ behavior: "smooth" });
    }
</script>

<svelte:window onkeydown={onKeydown} />

<header class="hero">
    <h1>The everyday tools, always at hand.</h1>
    <p class="dim">
        Decode, convert, hash, inspect, resize — the small jobs that come up all the time, in one
        fast place. And because everything runs in your browser, your data never leaves your
        device. <a href="#/trust">Verify it yourself.</a>
    </p>
    <input
        type="text"
        placeholder="Search {toolCount} tools… (press / to jump here)"
        bind:value={query}
        bind:this={searchBox}
        onkeydown={onSearchKeydown}
    />
    <div class="jump">
        {#each catalog.packs as pack (pack.name)}
            <button class="chip" style:--chip-color={meta(pack).color} onclick={() => jumpTo(pack.name)}>
                <span class="swatch" aria-hidden="true"></span>{meta(pack).title}
            </button>
        {/each}
    </div>
    <p class="cli-strip dim">
        Prefer the terminal? All of this is also <strong>one static binary</strong> with no
        network code — <a href="#/cli">install the CLI</a>.
    </p>
</header>

{#each filtered as pack (pack.name)}
    {#if pack.tools.length}
        <section id="pack-{pack.name}" style:--pack-color={meta(pack).color}>
            <h2><span class="swatch" aria-hidden="true"></span>{meta(pack).title}</h2>
            {#if meta(pack).blurb}
                <p class="blurb dim">{meta(pack).blurb}</p>
            {/if}
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

{#if query.trim() && matches === 0}
    <p class="dim none">
        Nothing matches “{query.trim()}”. Try a format name (base64, jwt, yaml) or a job (resize,
        hash, diff) — or <a href="#/builder">compose tools into a chain</a>.
    </p>
{/if}

<style>
    .hero {
        margin: 1.5rem 0 2rem;
        max-width: 42rem;
    }
    .hero input {
        margin-top: 0.8rem;
    }
    .jump {
        display: flex;
        flex-wrap: wrap;
        gap: 0.5rem;
        margin-top: 0.7rem;
    }
    .chip {
        display: inline-flex;
        align-items: center;
        gap: 0.4rem;
        background: none;
        border: 1px solid var(--border);
        border-radius: 999px;
        color: var(--text-dim);
        font-size: 0.82rem;
        padding: 0.25rem 0.7rem;
        cursor: pointer;
    }
    .chip:hover {
        color: var(--text);
        border-color: var(--chip-color);
    }
    .chip .swatch {
        background: var(--chip-color);
    }
    .swatch {
        display: inline-block;
        width: 0.55rem;
        height: 0.55rem;
        border-radius: 50%;
        background: var(--pack-color);
        flex-shrink: 0;
    }
    .cli-strip {
        margin: 0.9rem 0 0;
        font-size: 0.85rem;
    }
    section {
        margin-top: 1.6rem;
        scroll-margin-top: 1rem;
    }
    section h2 {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        margin-bottom: 0.15rem;
    }
    .blurb {
        margin: 0 0 0.8rem;
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
        border-left: 3px solid var(--pack-color);
        transition:
            border-color 0.12s,
            transform 0.12s,
            box-shadow 0.12s;
    }
    .tool:hover {
        text-decoration: none;
        border-color: var(--pack-color);
        transform: translateY(-1px);
        box-shadow: 0 3px 12px rgba(0, 0, 0, 0.18);
    }
    .types {
        font-size: 0.75rem;
    }
    .desc {
        font-size: 0.82rem;
    }
    .none {
        margin-top: 1.5rem;
    }
</style>
