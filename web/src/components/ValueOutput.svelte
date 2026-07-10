<script>
    import { prettySize, valueText } from "../lib/catalog.js";

    // Renders a tool's output {type, bytes, format?} with copy/download.
    let { value } = $props();

    let objectUrl = null;
    let imageUrl = $derived.by(() => {
        if (objectUrl) URL.revokeObjectURL(objectUrl);
        objectUrl = null;
        if (value?.type === "image") {
            const mime = value.format ? `image/${value.format}` : "application/octet-stream";
            objectUrl = URL.createObjectURL(new Blob([value.bytes], { type: mime }));
        }
        return objectUrl;
    });

    let display = $derived.by(() => {
        if (!value || value.type === "image") return "";
        if (value.type === "json") {
            try {
                return JSON.stringify(JSON.parse(valueText(value)), null, 2);
            } catch {
                return valueText(value);
            }
        }
        if (value.type === "bytes") {
            // Show bytes as text when they decode cleanly, else a hex preview.
            const decoded = new TextDecoder("utf-8", { fatal: false }).decode(value.bytes);
            const looksBinary =
                value.bytes.slice(0, 4096).some((b) => b < 9) || decoded.includes("�");
            if (!looksBinary) return decoded;
            const head = [...value.bytes.slice(0, 512)]
                .map((b) => b.toString(16).padStart(2, "0"))
                .join(" ");
            return head + (value.bytes.length > 512 ? " …" : "");
        }
        return valueText(value);
    });

    let copied = $state(false);

    async function copy() {
        await navigator.clipboard.writeText(display);
        copied = true;
        setTimeout(() => (copied = false), 1200);
    }

    function download() {
        const ext =
            value.type === "image"
                ? value.format || "img"
                : { text: "txt", json: "json", bytes: "bin" }[value.type];
        const a = document.createElement("a");
        a.href = URL.createObjectURL(new Blob([value.bytes]));
        a.download = `output.${ext}`;
        a.click();
        URL.revokeObjectURL(a.href);
    }
</script>

{#if value}
    <div class="output">
        {#if value.type === "image"}
            <img src={imageUrl} alt="tool output" />
        {:else}
            <pre>{display}</pre>
        {/if}
        <div class="bar">
            <span class="dim">{value.type}{value.format ? ` (${value.format})` : ""} · {prettySize(value.bytes.length)}</span>
            <span class="actions">
                {#if value.type !== "image"}
                    <button class="btn secondary" onclick={copy}>{copied ? "Copied ✓" : "Copy"}</button>
                {/if}
                <button class="btn secondary" onclick={download}>Download</button>
            </span>
        </div>
    </div>
{/if}

<style>
    .output {
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }
    pre {
        margin: 0;
        background: var(--bg-input);
        border: 1px solid var(--border);
        border-radius: var(--radius);
        padding: 0.7rem 0.9rem;
        max-height: 24rem;
        overflow: auto;
        white-space: pre-wrap;
        word-break: break-all;
        font-size: 0.85rem;
    }
    img {
        max-width: 100%;
        max-height: 24rem;
        object-fit: contain;
        border: 1px solid var(--border);
        border-radius: var(--radius);
        background:
            repeating-conic-gradient(#1e293b 0% 25%, #0f172a 0% 50%) 0 0 / 20px 20px;
    }
    .bar {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 0.6rem;
    }
    .actions {
        display: flex;
        gap: 0.5rem;
    }
</style>
