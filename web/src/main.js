import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";

const app = mount(App, { target: document.getElementById("app") });

// Offline support. Skipped in dev so the worker's cache never masks the dev
// server's live modules.
if ("serviceWorker" in navigator && !import.meta.env.DEV) {
    navigator.serviceWorker.register("sw.js");
    // The first page load happens before the worker controls the page, so
    // nothing from it lands in the cache — tell the worker to precache the
    // shell explicitly, or "offline after the first visit" is only true
    // from the second visit on.
    navigator.serviceWorker.ready.then((registration) => {
        const assets = [...document.querySelectorAll(
            "script[src], link[rel=stylesheet][href], link[rel=manifest][href], link[rel=icon][href], link[rel=apple-touch-icon][href]",
        )].map((el) => el.getAttribute("src") ?? el.getAttribute("href"));
        // manifests.json is fetched during this same uncontrolled first
        // boot, so it needs precaching too or the catalog fails offline.
        const urls = ["./", "wasm/manifests.json", ...assets]
            .map((u) => new URL(u, location.href).href)
            .filter((u) => u.startsWith(location.origin)); // data: favicons etc.
        registration.active?.postMessage({ type: "precache", urls });
    });
}

export default app;
