import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";

const app = mount(App, { target: document.getElementById("app") });

// Offline support. Skipped in dev so the worker's cache never masks the dev
// server's live modules.
if ("serviceWorker" in navigator && !import.meta.env.DEV) {
    navigator.serviceWorker.register("sw.js");
}

export default app;
