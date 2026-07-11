<script>
    import CliCommand from "../components/CliCommand.svelte";

    const REPO = "https://github.com/koundinyagoparaju/toolkit";
    const RAW = "https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main";

    // Purely client-side OS sniff to put the right command first — both
    // stay visible (people grab commands for other machines too).
    const platform = navigator.userAgentData?.platform ?? navigator.platform ?? "";
    const isWindows = /win/i.test(platform);

    const installers = [
        {
            os: "Linux / macOS",
            detected: !isWindows,
            command: `curl -fsSL ${RAW}/scripts/install.sh | sh`,
        },
        {
            os: "Windows (PowerShell)",
            detected: isWindows,
            command: `irm ${RAW}/scripts/install.ps1 | iex`,
        },
    ].sort((a, b) => b.detected - a.detected);
</script>

<h1>The CLI</h1>
<p class="dim">
    Every tool and chain on this site, as one static binary for your terminal — pipe-friendly,
    scriptable, offline, and streaming (gigabytes flow through in a few MB of memory). It is just
    as private as the website: the binary deliberately contains
    <strong>no network code at all</strong>, so it can't phone home even in principle.
</p>

<section>
    <h2>Install</h2>
    <p class="dim">
        One line — downloads the latest release for your platform and verifies its SHA-256
        checksum:
    </p>
    {#each installers as installer (installer.os)}
        <h3 class="dim">
            {installer.os}
            {#if installer.detected}<span class="detected">your system</span>{/if}
        </h3>
        <CliCommand command={installer.command} />
    {/each}
    <p class="dim">
        Updating later is the same one-liner — it compares your installed version to the latest
        release and only downloads when there's something new.
    </p>
    <p class="dim">
        Piping a script into your shell means trusting it — and this project's whole point is
        that you shouldn't have to. Each script is ~90 auditable lines: read
        <a href="{REPO}/blob/main/scripts/install.sh" target="_blank" rel="noreferrer"
            >install.sh</a
        >
        /
        <a href="{REPO}/blob/main/scripts/install.ps1" target="_blank" rel="noreferrer"
            >install.ps1</a
        >
        first, download a release
        <a href="{REPO}/releases" target="_blank" rel="noreferrer">yourself</a>, or build from
        the audited source with <code>cargo build --release -p toolkit-cli</code>. Releases ship
        with provenance attestations you can check with
        <code>gh attestation verify</code>.
    </p>
</section>

<section>
    <h2>A taste</h2>
    <pre>toolkit list                                  # every tool, with its types
toolkit run base64-encode 'hello world'       # input as an argument, or stdin
echo -n "$JWT" | toolkit chain 'jwt-decode | json-format'
toolkit run image-resize --set width=800 -i photo.png -o small.png
toolkit chain -n image-web-ready --set width=800 -i photo.png -o photo.jpg
toolkit chain -n text-compare -i old=a.txt -i new=b.txt
toolkit chain -n file-checksum -i backup.iso  # streams: GBs in a few MB of RAM</pre>
    <p class="dim">
        Every tool page here shows its own terminal equivalent — options included — and every
        chain in the <a href="#/chains">library</a> runs as
        <code>toolkit chain -n &lt;name&gt;</code>. Tab completion (tool names included) comes
        from <code>toolkit completions bash|zsh|fish|powershell</code>. Drop your own chain files into
        <code>~/.config/toolkit/chains/</code> and they run by name too.
    </p>
</section>

<style>
    h1 + p {
        max-width: 46rem;
    }
    section {
        max-width: 46rem;
        margin-top: 1.6rem;
    }
    h3 {
        margin: 0.9rem 0 0.4rem;
        font-size: 0.85rem;
        font-weight: 600;
    }
    .detected {
        margin-left: 0.4rem;
        padding: 0.1rem 0.45rem;
        border: 1px solid var(--accent-dim);
        border-radius: 999px;
        color: var(--accent);
        font-size: 0.7rem;
        font-weight: 600;
    }
    section p {
        margin-top: 0.8rem;
        font-size: 0.9rem;
    }
    pre {
        background: var(--bg-input);
        border: 1px solid var(--border);
        border-radius: var(--radius);
        padding: 0.8rem;
        font-size: 0.82rem;
        overflow-x: auto;
    }
</style>
