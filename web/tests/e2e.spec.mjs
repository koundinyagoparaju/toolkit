// End-to-end browser suite (Playwright, chromium) against a served build
// of the web app. Proves the load-bearing claims: wasm executes under the
// strict CSP, the CSP blocks exfiltration, chains/params/multi-ports/
// streaming/entropy all work in a real browser.
//
//   ./scripts/build-web-assets.sh && cd web && npm run build
//   npx playwright test                  # from web/ (starts the preview server itself)
import { expect, test } from "@playwright/test";
import { createHash } from "node:crypto";
import { readFileSync, statSync } from "node:fs";

/** Shareable-URL encoding of a chain: unpadded url-safe base64. */
const builderUrl = (chain) => `/#/builder/${Buffer.from(JSON.stringify(chain)).toString("base64url")}`;

/** Build a 40MB File inside the page and feed it to the file input.
 *  Constructed in-page (not setInputFiles) so nothing crosses the
 *  protocol; 40MB > the 32MB eager limit, so the streaming path runs. */
const feed40MB = (page) =>
    page.evaluate(() => {
        const mb = 1024 * 1024;
        const pattern = new Uint8Array(mb);
        for (let i = 0; i < mb; i++) pattern[i] = i % 251;
        const parts = Array.from({ length: 40 }, () => pattern);
        const file = new File(parts, "big.bin");
        const inputEl = document.querySelector('input[type="file"]');
        const dt = new DataTransfer();
        dt.items.add(file);
        inputEl.files = dt.files;
        inputEl.dispatchEvent(new Event("change", { bubbles: true }));
    });

/** sha256 of the same 40MB pattern, computed in Node as the oracle. */
function pattern40MBSha256() {
    const mb = 1024 * 1024;
    const pattern = Buffer.alloc(mb);
    for (let i = 0; i < mb; i++) pattern[i] = i % 251;
    const hasher = createHash("sha256");
    for (let i = 0; i < 40; i++) hasher.update(pattern);
    return hasher.digest("hex");
}

const swControlled = (page) =>
    page.waitForFunction(() => !!navigator.serviceWorker?.controller, undefined, {
        timeout: 60_000,
    });

test("tool page runs wasm, re-runs on edits and option changes", async ({ page }) => {
    await page.goto("/#/tool/hash");
    await page.locator("textarea").fill("abc");
    await expect(page.locator("pre")).toHaveText(
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );

    // Regression: EDITING an existing input must re-run the tool. The
    // auto-run effect once tracked only the containers, so it fired on the
    // first empty→filled transition and then went stale.
    await page.locator("textarea").fill("abcd");
    await expect(page.locator("pre")).toHaveText(
        "88d4266fd4e6338d13b845fcf289579d209c897823b9217da3e161936f031589",
    );

    // Regression: changing an option must re-run too.
    await page
        .locator("select", { has: page.locator('option[value="md5"]') })
        .selectOption("md5");
    await expect(page.locator("pre")).toHaveText("e2fc714c4727ee9395f324cd2e7f331f");
});

test("CSP blocks outbound fetch to other origins", async ({ page }) => {
    await page.goto("/#/tool/hash");
    const cspBlocked = await page.evaluate(() =>
        fetch("https://example.com").then(
            () => "allowed",
            () => "blocked",
        ),
    );
    expect(cspBlocked).toBe("blocked");
});

test("shared 2-node chain executes in the builder", async ({ page }) => {
    await page.goto(
        builderUrl({
            version: 1,
            nodes: [
                { id: "a", tool: "base64-decode", options: {} },
                { id: "b", tool: "json-format", options: { indent: 2 } },
            ],
            edges: [{ from: "a", to: "b" }],
        }),
    );
    await expect(page.locator("svg g rect")).toHaveCount(2);

    await page.locator("textarea").fill("eyJoZWxsbyI6IndvcmxkIn0="); // {"hello":"world"}
    await page.getByRole("button", { name: "Run chain" }).click();
    await expect(page.getByText("Chain ran ✓")).toBeVisible();

    // Click the sink node and check its output panel.
    await page.locator("svg g rect").nth(1).dispatchEvent("pointerdown");
    await expect(page.locator("aside pre")).toContainText('"hello": "world"');
});

test("multi-input tool page renders one input per port", async ({ page }) => {
    await page.goto("/#/tool/image-merge");
    const panels = page.locator("h2", { hasText: "Input" });
    await expect(panels).toHaveCount(2);
    await expect(panels.nth(0)).toContainText("first");
    await expect(panels.nth(1)).toContainText("second");
});

test("library chain with params shows the settings panel", async ({ page, request }) => {
    const chain = await (await request.get("/chains/image-web-ready.json")).json();
    await page.goto(builderUrl(chain));
    await expect(page.locator("h2", { hasText: "Chain settings" })).toBeVisible();
    await expect(page.locator("label", { hasText: "Max width" })).toBeVisible();
    await expect(page.locator("label", { hasText: "JPEG quality" })).toBeVisible();
});

test("fan-in into a multi port, ordered by edge declaration", async ({ page }) => {
    await page.goto(
        builderUrl({
            version: 1,
            nodes: [
                { id: "hex", tool: "hex-encode", options: {} },
                { id: "b64", tool: "base64-encode", options: {} },
                { id: "m", tool: "doc-merge", options: { separator: " + " } },
            ],
            edges: [
                { from: "hex", to: "m" },
                { from: "b64", to: "m" },
            ],
        }),
    );
    await page.locator("textarea").fill("hi");
    await page.getByRole("button", { name: "Run chain" }).click();
    await expect(page.getByText("Chain ran ✓")).toBeVisible();

    await page.locator("svg g rect").nth(2).dispatchEvent("pointerdown");
    await expect(page.locator("aside pre")).toHaveText("6869 + aGk=");
    await expect(page.locator("svg .badge text")).toHaveText(["1", "2"]);
});

test("40MB file streams through the hash tool", async ({ page }) => {
    await page.goto("/#/tool/hash");
    await page.locator('input[type="file"]').waitFor({ state: "attached" });
    await feed40MB(page);
    await expect(page.locator("pre")).toHaveText(pattern40MBSha256(), { timeout: 90_000 });
});

test("generator page auto-runs with browser entropy", async ({ page }) => {
    await page.goto("/#/tool/uuid");
    await expect(page.locator("pre")).toHaveText(
        /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
    // The entropy port is the driver's business, never a visible input.
    await expect(page.locator("h2", { hasText: "entropy" })).toHaveCount(0);
});

test("keyboard-only connecting in the builder", async ({ page }) => {
    // Two unconnected nodes; connect them with Enter on the output port
    // then Enter on the input port — no pointer events at all.
    await page.goto(
        builderUrl({
            version: 1,
            nodes: [
                { id: "a", tool: "base64-decode", options: {} },
                { id: "b", tool: "json-format", options: {} },
            ],
            edges: [],
        }),
    );
    await expect(page.locator("svg g rect")).toHaveCount(2);

    const outPort = page.locator("circle.out").first();
    await outPort.focus();
    await page.keyboard.press("Enter");
    await expect(outPort).toHaveAttribute("aria-pressed", "true");

    const inPort = page.locator("svg > g:not(.badge)").last().locator("circle:not(.out)");
    await inPort.focus();
    await page.keyboard.press("Enter");
    await expect(page.locator("path.edge")).toHaveCount(1);
});

test("chain library page opens a chain in the builder", async ({ page }) => {
    await page.goto("/#/chains");
    await page.getByRole("link", { name: /JWT claims/ }).click();
    await expect
        .poll(async () => page.locator("svg g rect").count(), { timeout: 45_000 })
        .toBeGreaterThanOrEqual(3);
});

test("chain with declared inputs feeds two named panels into a diff", async ({ page }) => {
    await page.goto(
        builderUrl({
            version: 1,
            inputs: [
                { name: "old", binds: [{ node: "diff", port: "old" }] },
                { name: "new", binds: [{ node: "diff", port: "new" }] },
            ],
            nodes: [{ id: "diff", tool: "text-diff", options: {} }],
            edges: [],
        }),
    );
    const panels = page.locator("h2", { hasText: "Input “" });
    await expect(panels).toHaveCount(2);
    await expect(panels.nth(0)).toContainText("old");
    await expect(panels.nth(1)).toContainText("new");

    await page.locator(".input-panel textarea").nth(0).fill("alpha\nbeta\n");
    await page.locator(".input-panel textarea").nth(1).fill("alpha\ngamma\n");
    await page.getByRole("button", { name: "Run chain" }).click();
    await expect(page.getByText("Chain ran ✓")).toBeVisible();

    await page.locator("svg g rect").first().dispatchEvent("pointerdown");
    await expect(page.locator("body")).toContainText("-beta");
    await expect(page.locator("body")).toContainText("+gamma");
});

test("service worker streams page-fed chunks as a download response", async ({ page }) => {
    await page.goto("/#/");
    await swControlled(page); // clients.claim() takes over shortly after first load
    const roundTrip = await page.evaluate(async () => {
        const token = crypto.randomUUID();
        const channel = new MessageChannel();
        const ready = new Promise((r) => (channel.port1.onmessage = (m) => m.data.ready && r()));
        navigator.serviceWorker.controller.postMessage(
            { type: "stream-download", token, filename: "t.txt", port: channel.port2 },
            [channel.port2],
        );
        await ready;
        const enc = new TextEncoder();
        const responsePromise = fetch("stream-download/" + token + "/t.txt");
        channel.port1.postMessage({ chunk: enc.encode("hello ").buffer });
        channel.port1.postMessage({ chunk: enc.encode("stream").buffer });
        channel.port1.postMessage({ done: true });
        const response = await responsePromise;
        return {
            body: await response.text(),
            disposition: response.headers.get("Content-Disposition"),
        };
    });
    expect(roundTrip.body).toBe("hello stream");
    expect(roundTrip.disposition).toContain('filename="t.txt"');
});

test("40MB chain output streams to a real file download", async ({ page }) => {
    await page.goto(
        builderUrl({
            version: 1,
            nodes: [{ id: "enc", tool: "base64-encode", options: {} }],
            edges: [],
        }),
    );
    await swControlled(page); // sink downloads flow through the service worker
    await page.locator('input[type="file"]').waitFor({ state: "attached" });
    await feed40MB(page);
    await page.locator(".stream-toggle input").waitFor({ state: "attached" });

    const downloadPromise = page.waitForEvent("download", { timeout: 120_000 });
    await page.getByRole("button", { name: "Run chain" }).click();
    await expect(page.getByText("Chain ran ✓")).toBeVisible({ timeout: 120_000 });

    // download.path() resolves once the file has fully landed on disk.
    const download = await downloadPromise;
    const file = await download.path();
    // base64 of 40MB, no line breaks: 4 * ceil(40MB / 3) bytes.
    expect(statSync(file).size).toBe(4 * Math.ceil((40 * 1024 * 1024) / 3));
});

test("wasm integrity: pins baked into the bundle, tampering detected", async ({
    page,
    request,
}) => {
    // The pins ship inside the app bundle (not fetched at runtime, so
    // they can't skew across deploys) and cover every pack.
    const pins = JSON.parse(
        readFileSync(new URL("../src/lib/wasm-integrity.json", import.meta.url), "utf8"),
    );
    for (const m of ["text.wasm", "image.wasm", "crypto.wasm", "data.wasm"]) {
        expect(pins[m]).toMatch(/^[0-9a-f]{64}$/);
    }

    await page.goto("/#/");
    const src = await page.locator('script[type="module"]').first().getAttribute("src");
    const bundle = await (await request.get(src)).text();
    for (const pin of Object.values(pins)) {
        expect(bundle).toContain(pin);
    }

    // The real digest must match its pin, and a flipped byte must not —
    // proving the loader's comparison would actually catch tampering.
    const verify = await page.evaluate(async (pin) => {
        const buf = await fetch("wasm/text.wasm").then((r) => r.arrayBuffer());
        const hex = (b) =>
            [...new Uint8Array(b)].map((x) => x.toString(16).padStart(2, "0")).join("");
        const real = hex(await crypto.subtle.digest("SHA-256", buf));
        const tampered = new Uint8Array(buf.slice(0));
        tampered[0] ^= 0xff;
        const bad = hex(await crypto.subtle.digest("SHA-256", tampered.buffer));
        return real === pin && bad !== pin ? "detects" : "MISS";
    }, pins["text.wasm"]);
    expect(verify).toBe("detects");
});
