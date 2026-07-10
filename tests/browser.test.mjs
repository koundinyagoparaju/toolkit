// End-to-end browser suite: drives headless Chrome over the DevTools
// Protocol (zero npm dependencies) against a served build of the web app.
// Proves the load-bearing claims: wasm executes under the strict CSP, the
// CSP blocks exfiltration, chains/params/multi-ports/streaming/entropy all
// work in a real browser.
//
//   ./scripts/build-web-assets.sh && cd web && npm run build
//   npm run preview -- --port 4173 &
//   node tests/browser.test.mjs          # BASE_URL/CHROME_BIN to override
import { execFile } from "node:child_process";

const CHROME = process.env.CHROME_BIN ?? "google-chrome";
const PORT = Number(process.env.CDP_PORT ?? 9222);
const BASE = process.env.BASE_URL ?? "http://localhost:4173";
const PROFILE = `/tmp/toolkit-cdp-${process.pid}`;

const chrome = execFile(CHROME, [
    "--headless=new",
    "--disable-gpu",
    "--no-sandbox",
    `--remote-debugging-port=${PORT}`,
    `--user-data-dir=${PROFILE}`,
    "about:blank",
]);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

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

async function waitFor(cdp, expression, timeoutMs = 15000) {
    const start = Date.now();
    for (;;) {
        if (await evalJs(cdp, expression)) return;
        if (Date.now() - start > timeoutMs) throw new Error(`timeout waiting for: ${expression}`);
        await sleep(300);
    }
}

const assert = (cond, msg) => {
    if (!cond) {
        console.error(`FAIL: ${msg}`);
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
    await sleep(1500); // debounce (250ms) + wasm fetch + run

    const hashOut = await evalJs(cdp, `document.querySelector("pre")?.textContent ?? ""`);
    assert(
        hashOut.trim() === "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        `hash tool computes sha256 in the browser (got "${hashOut.trim().slice(0, 40)}…")`,
    );

    // CSP: outbound requests must be blocked.
    const cspBlocked = await evalJs(
        cdp,
        `fetch("https://example.com").then(() => "allowed").catch(() => "blocked")`,
    );
    assert(cspBlocked === "blocked", "CSP blocks outbound fetch to other origins");

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
    await sleep(2000);

    const ranOk = await evalJs(cdp2, `document.body.textContent.includes("Chain ran ✓")`);
    assert(ranOk, "2-node chain (base64-decode → json-format) runs in the browser");

    // Click the sink node and check its output panel.
    await evalJs(
        cdp2,
        `(() => {
            const rects = document.querySelectorAll("svg g rect");
            rects[1].dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
        })()`,
    );
    await sleep(500);
    const sideOut = await evalJs(cdp2, `document.querySelector("aside pre")?.textContent ?? ""`);
    assert(
        sideOut.includes('"hello": "world"'),
        `sink node shows formatted JSON (got "${sideOut.slice(0, 60)}")`,
    );


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
    await sleep(2000);
    const fanOk = await evalJs(cdp5, `document.body.textContent.includes("Chain ran ✓")`);
    assert(fanOk, "fan-in chain (hex + base64 → doc-merge) runs in the browser");
    await evalJs(cdp5, `(() => {
        const rects = document.querySelectorAll("svg g rect");
        rects[2].dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    })()`);
    await sleep(500);
    const fanOut = await evalJs(cdp5, `document.querySelector("aside pre")?.textContent ?? ""`);
    assert(
        fanOut.trim() === "6869 + aGk=",
        `doc-merge output ordered by edge declaration (got "${fanOut.trim()}")`,
    );
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
    let digest6 = "";
    for (let i = 0; i < 30; i++) {
        await sleep(1000);
        digest6 = (await evalJs(cdp6, `document.querySelector("pre")?.textContent ?? ""`)).trim();
        if (/^[0-9a-f]{64}$/.test(digest6)) break;
    }
    assert(
        digest6 === expected,
        `40MB file streamed through hash in-browser (got "${digest6.slice(0, 16)}…", want "${expected.slice(0, 16)}…")`,
    );
    cdp6.close();


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

    cdp.close();
    cdp2.close();
} finally {
    chrome.kill();
}
console.log(process.exitCode ? "\nSome browser tests FAILED." : "\nAll browser tests passed.");
