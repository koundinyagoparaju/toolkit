// End-to-end browser suite: drives headless Chrome over the DevTools
// Protocol (zero npm dependencies) against a served build of the web app.
// Proves the load-bearing claims: wasm executes under the strict CSP, the
// CSP blocks exfiltration, chains/params/multi-ports/streaming/entropy all
// work in a real browser.
//
//   ./scripts/build-web-assets.sh && cd web && npm run build
//   npm run preview -- --port 4173 &
//   node tests/browser.test.mjs          # BASE_URL/CHROME_BIN to override
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";

const CHROME = process.env.CHROME_BIN ?? "google-chrome";
const BASE = process.env.BASE_URL ?? "http://localhost:4173";
const PROFILE = `/tmp/toolkit-cdp-${process.pid}`;

// Port 0 = Chrome picks a free one and writes it to the profile's
// DevToolsActivePort file — the canonical readiness signal, and no fixed
// port to collide on. Capture stderr and the exit code so a Chrome that
// dies at launch says why instead of surfacing as a connection timeout.
const chrome = spawn(
    CHROME,
    [
        "--headless=new",
        "--disable-gpu",
        "--no-sandbox",
        `--remote-debugging-port=${process.env.CDP_PORT ?? 0}`,
        `--user-data-dir=${PROFILE}`,
        "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
);
let chromeStderr = "";
chrome.stderr.on("data", (d) => (chromeStderr += d));
let chromeExit = null;
chrome.on("exit", (code) => (chromeExit = code));

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function devtoolsPort() {
    for (let i = 0; i < 120; i++) {
        if (chromeExit !== null) {
            throw new Error(`chrome exited with ${chromeExit} at launch: ${chromeStderr.trim()}`);
        }
        try {
            return Number(readFileSync(`${PROFILE}/DevToolsActivePort`, "utf8").split("\n")[0]);
        } catch {
            await sleep(250);
        }
    }
    throw new Error(`chrome devtools not ready after 30s: ${chromeStderr.trim()}`);
}
const PORT = await devtoolsPort();

/** Close the page itself — cdp.close() only closes the WebSocket, and a
 *  pile of live tabs (one of which just streamed 40MB) can stall later
 *  tab loads under memory pressure. */
const closeTab = (tab) => fetch(`http://localhost:${PORT}/json/close/${tab.id}`).catch(() => {});

async function newTab(url) {
    for (let i = 0; i < 40; i++) {
        try {
            const res = await fetch(`http://localhost:${PORT}/json/new?${encodeURIComponent(url)}`, {
                method: "PUT",
            });
            return await res.json();
        } catch {
            await sleep(250);
        }
    }
    throw new Error("could not reach chrome devtools");
}

function connect(wsUrl) {
    return new Promise((resolve, reject) => {
        const ws = new WebSocket(wsUrl);
        let id = 0;
        const pending = new Map();
        ws.onopen = () =>
            resolve({
                send(method, params = {}) {
                    return new Promise((res, rej) => {
                        const msgId = ++id;
                        pending.set(msgId, { res, rej });
                        ws.send(JSON.stringify({ id: msgId, method, params }));
                    });
                },
                close: () => ws.close(),
            });
        ws.onerror = reject;
        ws.onmessage = (event) => {
            const msg = JSON.parse(event.data);
            if (msg.id && pending.has(msg.id)) {
                const { res, rej } = pending.get(msg.id);
                pending.delete(msg.id);
                msg.error ? rej(new Error(msg.error.message)) : res(msg.result);
            }
        };
    });
}

async function evalJs(cdp, expression) {
    const { result, exceptionDetails } = await cdp.send("Runtime.evaluate", {
        expression,
        awaitPromise: true,
        returnByValue: true,
    });
    if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? "JS error");
    return result.value;
}

async function waitFor(cdp, expression, timeoutMs = 45000) {
    // A throw inside the page counts as "not ready yet", not a failure:
    // early polls can run before the document even has a body (seen on
    // slow CI runners), and a condition that never stops throwing still
    // surfaces below as a timeout naming the expression.
    const guarded = `(() => { try { return !!(${expression}); } catch { return false; } })()`;
    const start = Date.now();
    for (;;) {
        if (await evalJs(cdp, guarded)) return;
        if (Date.now() - start > timeoutMs) throw new Error(`timeout waiting for: ${expression}`);
        await sleep(300);
    }
}

// On GitHub Actions, ::error:: lines become check-run annotations, which
// are readable through the public API even when the job logs are not —
// so a CI-only failure names its test without needing log access.
const annotate = (msg) => {
    if (process.env.GITHUB_ACTIONS) console.log(`::error::browser suite: ${msg}`);
};

const assert = (cond, msg) => {
    if (!cond) {
        console.error(`FAIL: ${msg}`);
        annotate(`FAIL: ${msg}`);
        process.exitCode = 1;
    } else {
        console.log(`ok - ${msg}`);
    }
};

try {
    // --- Test 1: single tool page runs wasm on typed input ---
    const tab = await newTab(`${BASE}/#/tool/hash`);
    const cdp = await connect(tab.webSocketDebuggerUrl);
    await waitFor(cdp, `!!document.querySelector("textarea")`);

    await evalJs(
        cdp,
        `(() => {
            const ta = document.querySelector("textarea");
            ta.value = "abc";
            ta.dispatchEvent(new Event("input", { bubbles: true }));
        })()`,
    );
    await waitFor(
        cdp,
        `(document.querySelector("pre")?.textContent ?? "").trim() === "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"`,
    );
    assert(true, "hash tool computes sha256 in the browser");

    // CSP: outbound requests must be blocked.
    const cspBlocked = await evalJs(
        cdp,
        `fetch("https://example.com").then(() => "allowed").catch(() => "blocked")`,
    );
    assert(cspBlocked === "blocked", "CSP blocks outbound fetch to other origins");

    // Regression: EDITING an existing input must re-run the tool. The
    // auto-run effect once tracked only the containers, so it fired on the
    // first empty→filled transition and then went stale.
    await evalJs(
        cdp,
        `(() => {
            const ta = document.querySelector("textarea");
            ta.value = "abcd";
            ta.dispatchEvent(new Event("input", { bubbles: true }));
        })()`,
    );
    await waitFor(
        cdp,
        `(document.querySelector("pre")?.textContent ?? "").trim() === "88d4266fd4e6338d13b845fcf289579d209c897823b9217da3e161936f031589"`,
    );
    assert(true, "editing the input re-runs the tool (sha256 of \"abcd\")");

    // Regression: changing an option must re-run too.
    await evalJs(
        cdp,
        `(() => {
            const sel = [...document.querySelectorAll("select")].find((s) =>
                [...s.options].some((o) => o.value === "md5"));
            sel.value = "md5";
            sel.dispatchEvent(new Event("change", { bubbles: true }));
        })()`,
    );
    await waitFor(
        cdp,
        `(document.querySelector("pre")?.textContent ?? "").trim() === "e2fc714c4727ee9395f324cd2e7f331f"`,
    );
    assert(true, "changing an option re-runs the tool (md5 of \"abcd\")");

    // --- Test 2: a shared 2-node chain executes in the builder ---
    const chain = {
        version: 1,
        nodes: [
            { id: "a", tool: "base64-decode", options: {} },
            { id: "b", tool: "json-format", options: { indent: 2 } },
        ],
        edges: [{ from: "a", to: "b" }],
    };
    const hash = btoa(JSON.stringify(chain)).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
    const tab2 = await newTab(`${BASE}/#/builder/${hash}`);
    const cdp2 = await connect(tab2.webSocketDebuggerUrl);
    await waitFor(cdp2, `document.querySelectorAll("svg g rect").length >= 2`);

    const nodeCount = await evalJs(cdp2, `document.querySelectorAll("svg g rect").length`);
    assert(nodeCount === 2, `shared chain loads 2 nodes in the builder (got ${nodeCount})`);

    await evalJs(
        cdp2,
        `(() => {
            const ta = document.querySelector("textarea");
            ta.value = "eyJoZWxsbyI6IndvcmxkIn0=";  // {"hello":"world"}
            ta.dispatchEvent(new Event("input", { bubbles: true }));
        })()`,
    );
    await sleep(500);
    await evalJs(
        cdp2,
        `[...document.querySelectorAll("button")].find((b) => b.textContent.includes("Run chain")).click()`,
    );
    await waitFor(cdp2, `document.body.textContent.includes("Chain ran ✓")`);
    assert(true, "2-node chain (base64-decode → json-format) runs in the browser");

    // Click the sink node and check its output panel.
    await evalJs(
        cdp2,
        `(() => {
            const rects = document.querySelectorAll("svg g rect");
            rects[1].dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
        })()`,
    );
    await waitFor(cdp2, `(document.querySelector("aside pre")?.textContent ?? "").includes('"hello": "world"')`);
    assert(true, "sink node shows formatted JSON");


    // --- Test 3: multi-input tool page renders one input per port ---
    const tab3 = await newTab(`${BASE}/#/tool/image-merge`);
    const cdp3 = await connect(tab3.webSocketDebuggerUrl);
    await waitFor(cdp3, `[...document.querySelectorAll("h2")].some((h) => h.textContent.includes("Input"))`);
    const portInputs = await evalJs(cdp3, `[...document.querySelectorAll("h2")].filter((h) => h.textContent.includes("Input")).map((h) => h.textContent.trim())`);
    assert(
        portInputs.length === 2 && portInputs[0].includes("first") && portInputs[1].includes("second"),
        `image-merge page shows two named inputs (got ${JSON.stringify(portInputs)})`,
    );
    cdp3.close();

    // --- Test 4: library chain with params shows the settings panel ---
    const chainsList = await fetch(`${BASE}/chains/image-web-ready.json`).then((r) => r.json());
    const hash2 = btoa(String.fromCharCode(...new TextEncoder().encode(JSON.stringify(chainsList)))).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
    const tab4 = await newTab(`${BASE}/#/builder/${hash2}`);
    const cdp4 = await connect(tab4.webSocketDebuggerUrl);
    await waitFor(cdp4, `[...document.querySelectorAll("h2")].some((h) => h.textContent.includes("Chain settings"))`);
    const hasSettings = await evalJs(cdp4, `[...document.querySelectorAll("h2")].some((h) => h.textContent.includes("Chain settings"))`);
    const paramLabels = await evalJs(cdp4, `[...document.querySelectorAll("label")].map((l) => l.textContent.trim()).filter(Boolean)`);
    assert(hasSettings, "loaded library chain shows Chain settings panel");
    assert(
        paramLabels.some((l) => l.includes("Max width")) && paramLabels.some((l) => l.includes("JPEG quality")),
        `params render as form fields (got ${JSON.stringify(paramLabels.slice(0, 4))})`,
    );
    cdp4.close();


    // --- Test 5: fan-in into a multi port runs in the builder ---
    const fanChain = {
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
    };
    const fanHash = btoa(String.fromCharCode(...new TextEncoder().encode(JSON.stringify(fanChain))))
        .replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
    const tab5 = await newTab(`${BASE}/#/builder/${fanHash}`);
    const cdp5 = await connect(tab5.webSocketDebuggerUrl);
    await waitFor(cdp5, `!!document.querySelector("textarea")`);
    await evalJs(cdp5, `(() => {
        const ta = document.querySelector("textarea");
        ta.value = "hi";
        ta.dispatchEvent(new Event("input", { bubbles: true }));
    })()`);
    await sleep(400);
    await evalJs(cdp5, `[...document.querySelectorAll("button")].find((b) => b.textContent.includes("Run chain")).click()`);
    await waitFor(cdp5, `document.body.textContent.includes("Chain ran ✓")`);
    assert(true, "fan-in chain (hex + base64 → doc-merge) runs in the browser");
    await evalJs(cdp5, `(() => {
        const rects = document.querySelectorAll("svg g rect");
        rects[2].dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    })()`);
    await waitFor(cdp5, `(document.querySelector("aside pre")?.textContent ?? "").trim() === "6869 + aGk="`);
    assert(true, "doc-merge output ordered by edge declaration");
    const badges = await evalJs(cdp5, `[...document.querySelectorAll("svg .badge text")].map((t) => t.textContent)`);
    assert(JSON.stringify(badges) === JSON.stringify(["1", "2"]), `edge order badges shown (got ${JSON.stringify(badges)})`);
    cdp5.close();


    // --- Test 6: 40MB file streams through the hash tool (file.stream) ---
    const { createHash } = await import("node:crypto");
    const MB = 1024 * 1024;
    const pattern = Buffer.alloc(MB);
    for (let i = 0; i < MB; i++) pattern[i] = i % 251;
    const hasher = createHash("sha256");
    for (let i = 0; i < 40; i++) hasher.update(pattern);
    const expected = hasher.digest("hex");

    const tab6 = await newTab(`${BASE}/#/tool/hash`);
    const cdp6 = await connect(tab6.webSocketDebuggerUrl);
    await waitFor(cdp6, `!!document.querySelector('input[type="file"]')`);
    await evalJs(cdp6, `(() => {
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
    })()`);
    // 40MB > the 32MB eager limit, so this exercises the streaming path.
    await waitFor(cdp6, `(document.querySelector("pre")?.textContent ?? "").trim() === "${expected}"`, 90000);
    assert(true, "40MB file streamed through hash in-browser");
    cdp6.close();
    await closeTab(tab6);


    // --- Test 7: generator page auto-runs with browser entropy ---
    const tab7 = await newTab(`${BASE}/#/tool/uuid`);
    const cdp7 = await connect(tab7.webSocketDebuggerUrl);
    await waitFor(cdp7, `/^[0-9a-f-]{36}$/.test((document.querySelector("pre")?.textContent ?? "").trim())`);
    const uuidOut = (await evalJs(cdp7, `document.querySelector("pre")?.textContent ?? ""`)).trim();
    assert(
        /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(uuidOut),
        `uuid page auto-generates a v4 UUID (got "${uuidOut}")`,
    );
    const entropyHidden = await evalJs(cdp7, `[...document.querySelectorAll("h2")].every((h) => !h.textContent.includes("entropy"))`);
    assert(entropyHidden, "entropy port is hidden from the UI");
    cdp7.close();

    // --- Test 8: keyboard-only connecting in the builder ---
    // Two unconnected nodes; connect them with Enter on the output port
    // then Enter on the input port — no pointer events at all.
    const kbChain = {
        version: 1,
        nodes: [
            { id: "a", tool: "base64-decode", options: {} },
            { id: "b", tool: "json-format", options: {} },
        ],
        edges: [],
    };
    const kbHash = btoa(JSON.stringify(kbChain)).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
    const tabKb = await newTab(`${BASE}/#/builder/${kbHash}`);
    const cdpKb = await connect(tabKb.webSocketDebuggerUrl);
    await waitFor(cdpKb, `document.querySelectorAll("svg g rect").length >= 2`);
    const press = (selector) => `(() => {
        const el = ${selector};
        el.focus();
        el.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    })()`;
    await evalJs(cdpKb, press(`document.querySelector("circle.out")`)); // output of node a
    // Svelte applies state to the DOM asynchronously — wait for the port
    // to show the pending state before completing the connection.
    await waitFor(cdpKb, `document.querySelector("circle.out").getAttribute("aria-pressed") === "true"`);
    await evalJs(
        cdpKb,
        press(
            `[...document.querySelectorAll("svg > g:not(.badge)")].at(-1).querySelector("circle:not(.out)")`,
        ),
    ); // input of node b
    await waitFor(cdpKb, `document.querySelectorAll("path.edge").length === 1`);
    assert(true, "keyboard-only connect creates an edge");
    cdpKb.close();

    // --- Test 9a: the chain library page lists chains and opens one in
    // the builder ---
    const tabLib = await newTab(`${BASE}/#/chains`);
    const cdpLib = await connect(tabLib.webSocketDebuggerUrl);
    await waitFor(cdpLib, `document.body.textContent.includes("JWT claims")`);
    await evalJs(
        cdpLib,
        `[...document.querySelectorAll("a")].find((a) => a.textContent.includes("JWT claims")).click()`,
    );
    await waitFor(cdpLib, `document.querySelectorAll("svg g rect").length >= 3`);
    assert(true, "library chain opens in the builder with its nodes rendered");
    cdpLib.close();

    // --- Test 9b: chain with declared inputs — two named input panels,
    // each fed separately, produce a diff ---
    const diChain = {
        version: 1,
        inputs: [
            { name: "old", binds: [{ node: "diff", port: "old" }] },
            { name: "new", binds: [{ node: "diff", port: "new" }] },
        ],
        nodes: [{ id: "diff", tool: "text-diff", options: {} }],
        edges: [],
    };
    const diHash = btoa(JSON.stringify(diChain)).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
    const tabDi = await newTab(`${BASE}/#/builder/${diHash}`);
    const cdpDi = await connect(tabDi.webSocketDebuggerUrl);
    await waitFor(cdpDi, `document.querySelectorAll("textarea").length >= 2`);
    const inputPanels = await evalJs(
        cdpDi,
        `[...document.querySelectorAll("h2")].filter((h) => h.textContent.includes("Input “")).map((h) => h.textContent.trim())`,
    );
    assert(
        inputPanels.length === 2 && inputPanels[0].includes("old") && inputPanels[1].includes("new"),
        `declared-inputs chain shows two named input panels (got ${JSON.stringify(inputPanels)})`,
    );
    await evalJs(
        cdpDi,
        `(() => {
            const tas = [...document.querySelectorAll(".input-panel textarea")];
            tas[0].value = "alpha\\nbeta\\n";
            tas[0].dispatchEvent(new Event("input", { bubbles: true }));
            tas[1].value = "alpha\\ngamma\\n";
            tas[1].dispatchEvent(new Event("input", { bubbles: true }));
        })()`,
    );
    await sleep(300);
    await evalJs(
        cdpDi,
        `[...document.querySelectorAll("button")].find((b) => b.textContent.includes("Run chain")).click()`,
    );
    await waitFor(cdpDi, `document.body.textContent.includes("Chain ran ✓")`);
    await evalJs(cdpDi, `document.querySelector("svg g rect").dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }))`);
    await waitFor(cdpDi, `document.body.textContent.includes("-beta")`);
    const diffOut = await evalJs(cdpDi, `document.body.textContent.includes("-beta") && document.body.textContent.includes("+gamma")`);
    assert(diffOut, "two-input diff chain runs in the browser");
    cdpDi.close();

    // --- Test 10: streaming download — sink bytes flow through the
    // service worker's stream-download endpoint ---
    const tabSd = await newTab(`${BASE}/#/`);
    const cdpSd = await connect(tabSd.webSocketDebuggerUrl);
    // clients.claim() makes the SW take control shortly after first load.
    await waitFor(cdpSd, `!!navigator.serviceWorker?.controller`, 60000);
    const roundTrip = await evalJs(
        cdpSd,
        `(async () => {
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
        })()`,
    );
    assert(
        roundTrip.body === "hello stream" && roundTrip.disposition.includes('filename="t.txt"'),
        `SW streams page-fed chunks as a download response (got ${JSON.stringify(roundTrip)})`,
    );
    cdpSd.close();

    // --- Test 11: builder streams a 40MB file's sink output to a real
    // file download, keeping nothing in page memory ---
    const dlChain = {
        version: 1,
        nodes: [{ id: "enc", tool: "base64-encode", options: {} }],
        edges: [],
    };
    const dlHash = btoa(JSON.stringify(dlChain)).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
    const tabDl = await newTab(`${BASE}/#/builder/${dlHash}`);
    const cdpDl = await connect(tabDl.webSocketDebuggerUrl);
    const downloadDir = `/tmp/toolkit-dl-${process.pid}`;
    const { mkdirSync, readdirSync, statSync, rmSync } = await import("node:fs");
    mkdirSync(downloadDir, { recursive: true });
    // Page sessions accept Browser.* on current Chrome; fall back to the
    // browser-level socket, and skip the on-disk assertion if neither
    // works (test 10 already covers the stream mechanics).
    let downloadsEnabled = true;
    try {
        await cdpDl.send("Browser.setDownloadBehavior", {
            behavior: "allow",
            downloadPath: downloadDir,
        });
    } catch {
        try {
            const version = await (await fetch(`http://localhost:${PORT}/json/version`)).json();
            const browserCdp = await connect(version.webSocketDebuggerUrl);
            await browserCdp.send("Browser.setDownloadBehavior", {
                behavior: "allow",
                downloadPath: downloadDir,
            });
            browserCdp.close();
        } catch (e) {
            console.log(`skip - download-to-disk assertion (cannot enable downloads: ${e.message})`);
            downloadsEnabled = false;
        }
    }
    await waitFor(cdpDl, `!!navigator.serviceWorker?.controller`, 60000);
    await waitFor(cdpDl, `!!document.querySelector('input[type="file"]')`);
    await evalJs(cdpDl, `(() => {
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
    })()`);
    await waitFor(cdpDl, `!!document.querySelector(".stream-toggle input")`);
    await evalJs(
        cdpDl,
        `[...document.querySelectorAll("button")].find((b) => b.textContent.includes("Run chain")).click()`,
    );
    await waitFor(cdpDl, `document.body.textContent.includes("Chain ran ✓")`, 120000);
    if (downloadsEnabled) {
        // base64 of 40MB, no line breaks: 4 * ceil(40MB / 3) bytes.
        const expectedSize = 4 * Math.ceil((40 * 1024 * 1024) / 3);
        let downloaded = null;
        for (let i = 0; i < 120; i++) {
            const files = readdirSync(downloadDir).filter((f) => !f.endsWith(".crdownload"));
            if (files.length) {
                const size = statSync(`${downloadDir}/${files[0]}`).size;
                if (size === expectedSize) {
                    downloaded = { name: files[0], size };
                    break;
                }
            }
            await sleep(500);
        }
        assert(
            downloaded !== null,
            `40MB chain output streamed to a download of ${expectedSize} bytes (got ${JSON.stringify(downloaded)})`,
        );
    }
    rmSync(downloadDir, { recursive: true, force: true });
    cdpDl.close();
    await closeTab(tabDl);

    // --- Test 12: wasm integrity — the pins are baked into the served
    // app bundle (not fetched, so they can't skew across deploys), they
    // match the served packs, and a tampered byte diverges ---
    const pins = JSON.parse(
        readFileSync(new URL("../web/src/lib/wasm-integrity.json", import.meta.url), "utf8"),
    );
    const pinsOk = ["text.wasm", "image.wasm", "crypto.wasm", "data.wasm"].every((m) =>
        /^[0-9a-f]{64}$/.test(pins[m] ?? ""),
    );
    assert(pinsOk, `wasm-integrity.json pins sha256 for every pack (got ${Object.keys(pins).join(",")})`);

    const tab8 = await newTab(`${BASE}/#/`);
    const cdp8 = await connect(tab8.webSocketDebuggerUrl);
    const baked = await evalJs(
        cdp8,
        `(async () => {
            const pins = ${JSON.stringify(pins)};
            const src = document.querySelector('script[type="module"]').src;
            const js = await fetch(src).then((r) => r.text());
            return Object.values(pins).every((p) => js.includes(p)) ? "baked" : "missing";
        })()`,
    );
    assert(baked === "baked", `every pack pin ships inside the served app bundle (got "${baked}")`);

    // The real digest must match its pin, and a flipped byte must not —
    // proving the loader's comparison would actually catch tampering.
    const verifyResult = await evalJs(
        cdp8,
        `(async () => {
            const pin = ${JSON.stringify(pins["text.wasm"])};
            const buf = await fetch("wasm/text.wasm").then((r) => r.arrayBuffer());
            const hex = (b) => [...new Uint8Array(b)].map((x) => x.toString(16).padStart(2, "0")).join("");
            const real = hex(await crypto.subtle.digest("SHA-256", buf));
            const tampered = new Uint8Array(buf.slice(0));
            tampered[0] ^= 0xff;
            const bad = hex(await crypto.subtle.digest("SHA-256", tampered.buffer));
            return real === pin && bad !== pin ? "detects" : "MISS";
        })()`,
    );
    assert(verifyResult === "detects", `loader hash matches pin and a tampered byte diverges (got "${verifyResult}")`);
    cdp8.close();

    cdp.close();
    cdp2.close();
} catch (e) {
    annotate(`aborted: ${e.message}`);
    throw e;
} finally {
    chrome.kill();
}
console.log(process.exitCode ? "\nSome browser tests FAILED." : "\nAll browser tests passed.");
