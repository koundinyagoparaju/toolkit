<script>
    import { chainToHash, loadChainLibrary } from "../lib/catalog.js";
</script>

<h1>Chain library</h1>
<p class="dim">
    Reusable toolchains contributed by the community — reviewed in the open like every tool. A
    chain is pure data (which tools, in what order, with what settings); it can't do anything the
    audited tools can't. Open one in the builder to run or remix it, or
    <a href="#/builder">build your own</a>.
</p>
<p class="dim">
    Every chain here also runs in your terminal:
    <code>toolkit chain -n &lt;name&gt;</code> —
    <a href="https://github.com/koundinyagoparaju/toolkit#cli" target="_blank" rel="noreferrer"
        >install the CLI</a
    >.
</p>

{#await loadChainLibrary()}
    <p class="dim">Loading chains…</p>
{:then chains}
    <div class="grid">
        {#each chains as chain (chain.file)}
            <a class="card chain" href="#/builder/{chainToHash(chain)}">
                <strong>{chain.name}</strong>
                <span class="dim desc">{chain.description}</span>
                <span class="mono dim steps">
                    {chain.nodes.map((n) => n.tool).join(" → ")}
                </span>
            </a>
        {/each}
    </div>
{:catch error}
    <p class="error">Failed to load the chain library: {error.message}</p>
{/await}

<style>
    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
        gap: 0.8rem;
        margin-top: 1rem;
    }
    .chain {
        display: flex;
        flex-direction: column;
        gap: 0.3rem;
        color: var(--text);
    }
    .chain:hover {
        text-decoration: none;
        border-color: var(--accent-dim);
    }
    .desc {
        font-size: 0.85rem;
    }
    .steps {
        font-size: 0.75rem;
    }
</style>
