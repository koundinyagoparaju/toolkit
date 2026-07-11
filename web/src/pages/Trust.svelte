<h1>Why you can trust this site with your data</h1>

<p>
    You shouldn't have to take our word for it — every claim below is one you can verify yourself,
    right now, in your browser.
</p>

<section class="card">
    <h2>1. There is no server to send your data to</h2>
    <p>
        This site is a folder of static files. Your input is processed by WebAssembly modules
        running inside your browser tab. Nothing you type, paste, or drop here is transmitted
        anywhere.
    </p>
    <p class="dim">
        Verify: open DevTools → Network, then use any tool with any file. After the page and its
        tool modules load, you will see <strong>zero</strong> outgoing requests.
    </p>
</section>

<section class="card">
    <h2>2. The browser itself blocks exfiltration</h2>
    <p>
        The site ships a strict <code>Content-Security-Policy</code> with
        <code>connect-src 'self'</code> and <code>default-src 'none'</code>. Even if this site's
        code were malicious or compromised, the browser would refuse to let it contact any other
        server. The guarantee doesn't rest on our code being good — it rests on your browser
        enforcing the policy.
    </p>
    <p class="dim">
        Verify: DevTools → Network → click the document request → Headers, or run
        <code>fetch("https://example.com")</code> in the Console and watch the browser reject it.
    </p>
</section>

<section class="card">
    <h2>3. It works with the network unplugged</h2>
    <p>
        After your first visit the site is cached locally and runs fully offline. Turn on airplane
        mode and everything keeps working — the clearest possible proof that your data isn't going
        anywhere.
    </p>
</section>

<section class="card">
    <h2>4. Every line is open to audit</h2>
    <p>
        The tools are written in Rust with a minimal, pinned, vendorable dependency set and
        compiled to WebAssembly; the page around them is a small Svelte app. The source, the build
        pipeline, and this page are all in the public repository — and community tools and chains
        enter through reviewed pull requests only. The site never loads code from anywhere else.
    </p>
</section>

<section class="card">
    <h2>Prefer the terminal?</h2>
    <p>
        The same tools ship as a single static CLI binary with no network code at all:
        <code>echo 'eyJ…' | toolkit run-chain 'jwt-decode | json-format'</code> — or build it yourself
        from the audited source with <code>cargo install</code>.
    </p>
</section>

<style>
    section {
        margin-top: 1rem;
        max-width: 46rem;
    }
    h2 {
        margin-top: 0;
    }
</style>
