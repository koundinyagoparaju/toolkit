import { defineConfig } from "@playwright/test";

// Browser end-to-end suite (tests/e2e.spec.mjs).
// Prereqs: ../scripts/build-web-assets.sh && npm run build — the suite
// tests the served production build, and webServer starts it below.
export default defineConfig({
    testDir: "./tests",
    testMatch: "e2e.spec.mjs",
    // Two tests stream 40MB through wasm; give them room.
    timeout: 180_000,
    expect: { timeout: 45_000 },
    reporter: process.env.GITHUB_ACTIONS ? [["github"], ["list"]] : "list",
    use: {
        baseURL: "http://localhost:4173",
    },
    webServer: {
        command: "npm run preview -- --port 4173 --strictPort",
        url: "http://localhost:4173",
        reuseExistingServer: !process.env.CI,
    },
});
