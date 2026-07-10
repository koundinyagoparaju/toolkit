import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

// The privacy guarantee, enforced at the browser level: no connections to
// anything but our own origin (needed for lazy-loading wasm packs and the
// service worker), no eval, no embeds. Applied to the production build;
// dev mode needs inline styles/HMR websockets that this would block.
// Hosts should send the same policy as an HTTP header (see README).
const CSP = [
    "default-src 'none'",
    "script-src 'self' 'wasm-unsafe-eval'",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' blob: data:",
    "connect-src 'self'",
    "manifest-src 'self'",
    "worker-src 'self'",
    "base-uri 'none'",
    "form-action 'none'",
    "frame-ancestors 'none'",
].join("; ");

const injectCsp = {
    name: "inject-csp",
    apply: "build",
    transformIndexHtml(html) {
        return html.replace(
            "<!-- csp-placeholder -->",
            `<meta http-equiv="Content-Security-Policy" content="${CSP}">`,
        );
    },
};

export default defineConfig({
    plugins: [svelte(), injectCsp],
    build: { target: "es2022" },
});
