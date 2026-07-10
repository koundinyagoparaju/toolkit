<script>
    import { bytesValue, prettySize, textValue } from "../lib/catalog.js";

    // Input editor producing a {type, bytes} value. Text is typed or pasted;
    // any file can be dropped/picked (arrives as bytes — the tool coerces).
    // `hint` is the receiving tool's input type, used only to pick the
    // friendlier default UI (file-first for image/bytes tools).
    let { value = $bindable(), hint = "text" } = $props();

    // Only image tools default to the file picker; bytes tools (base64,
    // hex, hash) more often receive typed/pasted text.
    let fileFirst = $derived(hint === "image");
    let text = $state("");
    let fileInfo = $state(null); // {name, size}
    let dragging = $state(false);

    function fromText(t) {
        text = t;
        fileInfo = null;
        value = t === "" ? null : textValue(t);
    }

    // Files above this are not read eagerly: streaming tools consume them
    // chunk-by-chunk via file.stream(), buffered tools read on demand.
    const EAGER_LIMIT = 32 * 1024 * 1024;

    async function fromFile(file) {
        if (!file) return;
        text = "";
        fileInfo = { name: file.name, size: file.size };
        if (file.size <= EAGER_LIMIT) {
            const bytes = new Uint8Array(await file.arrayBuffer());
            value = { ...bytesValue(bytes), file };
        } else {
            value = { type: "bytes", bytes: null, file, size: file.size };
        }
    }

    function clear() {
        text = "";
        fileInfo = null;
        value = null;
    }
</script>

<div
    class="drop"
    class:dragging
    role="region"
    aria-label="input"
    ondragover={(e) => {
        e.preventDefault();
        dragging = true;
    }}
    ondragleave={() => (dragging = false)}
    ondrop={(e) => {
        e.preventDefault();
        dragging = false;
        fromFile(e.dataTransfer.files?.[0]);
    }}
>
    {#if fileInfo}
        <div class="file-info">
            <span>📄 {fileInfo.name} <span class="dim">({prettySize(fileInfo.size)})</span></span>
            <button class="btn secondary" onclick={clear}>Clear</button>
        </div>
    {:else if fileFirst}
        <label class="picker">
            <strong>Drop a file here</strong> or click to choose one.
            <em class="dim">It is processed on this device and never uploaded.</em>
            <input type="file" hidden onchange={(e) => fromFile(e.currentTarget.files?.[0])} />
        </label>
    {:else}
        <textarea
            placeholder="Paste or type input… (or drop a file)"
            value={text}
            oninput={(e) => fromText(e.currentTarget.value)}
        ></textarea>
        <label class="alt dim">
            or <span class="link">choose a file</span>
            <input type="file" hidden onchange={(e) => fromFile(e.currentTarget.files?.[0])} />
        </label>
    {/if}
</div>

<style>
    .drop {
        border-radius: var(--radius);
    }
    .drop.dragging {
        outline: 2px dashed var(--accent);
        outline-offset: 4px;
    }
    .picker {
        display: flex;
        flex-direction: column;
        gap: 0.3rem;
        align-items: center;
        justify-content: center;
        text-align: center;
        border: 2px dashed var(--border);
        border-radius: var(--radius);
        padding: 2.2rem 1rem;
        cursor: pointer;
        color: var(--text);
        font-size: 1rem;
    }
    .picker:hover {
        border-color: var(--accent-dim);
    }
    .file-info {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 1rem;
        border: 1px solid var(--border);
        border-radius: var(--radius);
        padding: 0.6rem 0.8rem;
    }
    .alt {
        margin-top: 0.35rem;
        font-size: 0.8rem;
        cursor: pointer;
    }
    .link {
        color: var(--accent);
    }
</style>
