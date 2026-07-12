<script>
    import { loadCatalog } from "./lib/catalog.js";
    import Builder from "./pages/Builder.svelte";
    import Chains from "./pages/Chains.svelte";
    import Cli from "./pages/Cli.svelte";
    import Home from "./pages/Home.svelte";
    import Tool from "./pages/Tool.svelte";
    import Trust from "./pages/Trust.svelte";

    function parseHash() {
        const hash = location.hash.replace(/^#\/?/, "");
        const [page, ...rest] = hash.split("/");
        return { page: page || "home", arg: rest.join("/") };
    }

    let route = $state(parseHash());
    window.addEventListener("hashchange", () => (route = parseHash()));

    // Theme: no stored choice = follow the OS (pure CSS, no flash); a
    // header click pins light or dark and persists it.
    let theme = $state(localStorage.getItem("toolkit-theme") ?? "");
    const osLight = matchMedia("(prefers-color-scheme: light)");
    let osIsLight = $state(osLight.matches);
    osLight.addEventListener("change", (e) => (osIsLight = e.matches));
    let effectiveLight = $derived(theme ? theme === "light" : osIsLight);
    $effect(() => {
        if (theme) document.documentElement.dataset.theme = theme;
        else delete document.documentElement.dataset.theme;
    });
    function toggleTheme() {
        theme = effectiveLight ? "dark" : "light";
        localStorage.setItem("toolkit-theme", theme);
    }
</script>

<nav>
    <div class="container nav-inner">
        <a class="brand" href="#/"><svg viewBox="0 0 512 512" width="20" height="20" aria-hidden="true" style="vertical-align: -0.28em"><path d="M186 208 v-30 a26 26 0 0 1 26-26 h88 a26 26 0 0 1 26 26 v30" fill="none" stroke="currentColor" stroke-width="34" stroke-linecap="round"/><rect x="112" y="208" width="288" height="164" rx="26" fill="none" stroke="currentColor" stroke-width="34"/><line x1="125" y1="278" x2="228" y2="278" stroke="currentColor" stroke-width="26"/><line x1="284" y1="278" x2="387" y2="278" stroke="currentColor" stroke-width="26"/><rect x="234" y="258" width="44" height="40" rx="10" fill="none" stroke="currentColor" stroke-width="26"/></svg> toolkit</a>
        <div class="links">
            <a href="#/">Tools</a>
            <a href="#/chains">Chains</a>
            <a href="#/builder">Builder</a>
            <a href="#/cli">CLI</a>
            <a href="#/trust">Why trust this?</a>
            <button
                class="theme-toggle"
                onclick={toggleTheme}
                title="Switch to {effectiveLight ? 'dark' : 'light'} theme"
                aria-label="Switch to {effectiveLight ? 'dark' : 'light'} theme"
            >
                {effectiveLight ? "☾" : "☀"}
            </button>
        </div>
    </div>
</nav>

<main class="container">
    {#await loadCatalog()}
        <p class="dim">Loading tool catalog…</p>
    {:then catalog}
        {#if route.page === "tool" && route.arg}
            <Tool {catalog} name={route.arg} />
        {:else if route.page === "chains"}
            <Chains />
        {:else if route.page === "builder"}
            <Builder {catalog} shared={route.arg} />
        {:else if route.page === "cli"}
            <Cli />
        {:else if route.page === "trust"}
            <Trust />
        {:else}
            <Home {catalog} />
        {/if}
    {:catch error}
        <p class="error">Failed to load the tool catalog: {error.message}</p>
    {/await}
</main>

<style>
    nav {
        border-bottom: 1px solid var(--border);
        background: var(--bg-raised);
    }
    .nav-inner {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding-top: 0.7rem;
        padding-bottom: 0.7rem;
    }
    .brand {
        font-weight: 700;
        font-size: 1.1rem;
        color: var(--text);
    }
    .links {
        display: flex;
        align-items: center;
        gap: 1.2rem;
    }
    .links a {
        color: var(--text-dim);
    }
    .links a:hover {
        color: var(--text);
        text-decoration: none;
    }
    .theme-toggle {
        background: none;
        border: 1px solid var(--border);
        border-radius: var(--radius);
        color: var(--text-dim);
        font-size: 0.95rem;
        line-height: 1;
        padding: 0.3rem 0.5rem;
        cursor: pointer;
    }
    .theme-toggle:hover {
        color: var(--text);
        border-color: var(--accent-dim);
    }
    /* Narrow screens: brand on its own line, links wrapping under it —
       never the brand orphaned below the links. */
    @media (max-width: 620px) {
        .nav-inner {
            flex-direction: column;
            align-items: flex-start;
            gap: 0.35rem;
        }
        .links {
            flex-wrap: wrap;
            gap: 0.9rem;
            row-gap: 0.25rem;
            font-size: 0.92rem;
        }
    }
</style>
