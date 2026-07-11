<script>
    // A copyable shell command — the web app's upsell for the CLI.
    let { command } = $props();
    let copied = $state(false);

    async function copy() {
        await navigator.clipboard.writeText(command);
        copied = true;
        setTimeout(() => (copied = false), 1500);
    }
</script>

<div class="cli">
    <code>{command}</code>
    <button class="btn secondary" onclick={copy}>{copied ? "Copied ✓" : "Copy"}</button>
</div>

<style>
    .cli {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        background: var(--bg-input);
        border: 1px solid var(--border);
        border-radius: var(--radius);
        padding: 0.45rem 0.45rem 0.45rem 0.8rem;
    }
    code {
        flex: 1;
        overflow-x: auto;
        white-space: nowrap;
        font-size: 0.82rem;
        color: var(--text-dim);
    }
    button {
        flex-shrink: 0;
    }
</style>
