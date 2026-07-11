# toolkit

**The everyday tools you keep reaching for — handy, fast, and private by construction.**

**Use it now: [koundinyagoparaju.github.io/toolkit](https://koundinyagoparaju.github.io/toolkit/)** — or install the CLI:

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main/scripts/install.sh | sh
# Windows (PowerShell)
irm https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main/scripts/install.ps1 | iex

echo -n "$JWT" | toolkit chain 'jwt-decode | json-format'
toolkit chain -n image-web-ready --set width=800 -i photo.png -o photo.jpg
toolkit chain -n file-checksum -i backup.iso    # streams: gigabytes in a few MB of RAM
```

The CLI is the same 52 tools and every chain as one static binary —
pipe-friendly, scriptable, offline, and it deliberately contains no
network code at all ([details](#cli)).

Base64/32/58, URL and hex encoding, JWT inspection, JSON ↔ YAML/TOML/CSV,
hashing and HMAC, timestamps, regex extraction, diffs, gzip, QR codes,
password/UUID generation, EXIF stripping, image resize/crop/convert/merge —
the small jobs that come up constantly, in one fast place, composable into
chains. And unlike the websites you'd otherwise paste sensitive data into,
**everything runs on your own device**: in your browser as WebAssembly, or
in your terminal as a single static binary. There is no server. There is
nothing to upload to.

## Why you can trust it (verify, don't believe)

| Claim | How you verify it |
| --- | --- |
| No server receives your data | The site is static files. Open DevTools → Network while using any tool: zero outgoing requests. |
| Even malicious code couldn't exfiltrate | A strict `Content-Security-Policy` (`default-src 'none'; connect-src 'self'`) makes the browser itself refuse outbound connections. Try `fetch("https://example.com")` in the console. |
| Works with the network unplugged | The site is an offline-capable PWA. Turn on airplane mode; everything keeps working. |
| The code is what you audited | Tools are pure Rust (pinned, vendorable, pure-Rust dependencies) compiled to wasm; the page is a small Svelte app; all of it in this repo. Community tools/chains enter via reviewed PRs only — the site never loads code from anywhere else. |
| The binaries match the source | Builds are reproducible (pinned toolchain, locked deps, normalized paths — enforced in CI) and releases carry GitHub provenance attestations. Rebuild and compare: [docs/architecture.md](docs/architecture.md#reproducing-a-release). |
| The CLI can't phone home | It contains no network code at all, and you can build it from source: `cargo install --path crates/cli`. |

## Using it

### Web

The app is live at
**[koundinyagoparaju.github.io/toolkit](https://koundinyagoparaju.github.io/toolkit/)**
— it works offline after the first visit, and you can verify its no-network
claims yourself (see the table above, or the in-app "Why trust this?" page).

![Tool catalog](docs/images/catalog.png)

Compose tools into typed pipelines in the chain builder — here two resize
branches fan into image-merge's named ports:

![Chain builder](docs/images/builder.png)

To run it locally:

```sh
./scripts/build-web-assets.sh   # compile tool packs to wasm + emit catalog
cd web && npm install && npm run dev
```

### CLI

One-line install — downloads the latest release for your platform and
verifies its SHA-256 checksum:

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main/scripts/install.sh | sh
# or:  wget -qO- https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main/scripts/install.sh | sh
```

```powershell
# Windows (PowerShell; installs to %LOCALAPPDATA%\toolkit\bin and adds it to your user PATH)
irm https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main/scripts/install.ps1 | iex
```

Or through a package manager — the tap and bucket live in this repo, so
what you install is exactly what you can read here:

```sh
# Homebrew (macOS/Linux)
brew tap koundinyagoparaju/toolkit https://github.com/koundinyagoparaju/toolkit
brew install koundinyagoparaju/toolkit/toolkit
```

```powershell
# Scoop (Windows)
scoop bucket add toolkit https://github.com/koundinyagoparaju/toolkit
scoop install toolkit
```

Piping a script into your shell means trusting it — and this project's
whole point is that you shouldn't have to. Each script is ~90 auditable
lines; download and read it first, or build from the audited source
instead:

```sh
cargo build --release -p toolkit-cli   # -> target/release/toolkit

toolkit list                                      # what's available
toolkit run base64-encode 'hello world'           # input as an argument…
echo -n 'hello' | toolkit run base64-encode       # …or from stdin
toolkit run image-resize --set width=800 -i in.png -o out.png
# multi-input tools take one file per named port:
toolkit run image-merge -i first=a.png -i second=b.png --set mode=vertical -o out.png
# variable-arity ports (marked "…" in `toolkit list`) take repeated -i:
toolkit run doc-merge -i a.txt -i b.txt -i c.txt --set separator=$'\n---\n'

# chains: pipe syntax…
echo "$JWT" | toolkit chain 'jwt-decode | json-format indent=4'
# …or the chain library (with declared, typed parameters):
toolkit chains                                    # browse the library
toolkit chain --name image-web-ready --set width=800 -i photo.png -o photo.jpg
toolkit chain --file my-chain.json -i input.txt
```

Shell completions for bash, zsh, fish, and PowerShell — including tool
names after `run`, and option keys and enum values after `--set`
(`toolkit run hash --set algorithm=<TAB>` offers the algorithms; bash,
zsh, and fish).
Put them at these paths and the installer refreshes them on every update:

```sh
toolkit completions zsh  > ~/.zsh/completions/_toolkit   # + add dir to fpath, run compinit
toolkit completions fish > ~/.config/fish/completions/toolkit.fish
toolkit completions bash > ~/.local/share/bash-completion/completions/toolkit
```

```powershell
toolkit completions powershell > "$env:LOCALAPPDATA\toolkit\completions.ps1"
Add-Content $PROFILE '. "$env:LOCALAPPDATA\toolkit\completions.ps1"'
```

Drop your own chain files into `~/.config/toolkit/chains/` and run them by
name — chains are pure data, so this needs no code trust. To update the CLI,
re-run the install one-liner (it detects your version) or use your package
manager; the `toolkit` binary itself deliberately contains no network
code, so it never updates itself.

## How it's put together

```
crates/core         the contract: typed values (text/bytes/json/image) with a
                    coercion matrix, tool manifests (options auto-generate web
                    forms and CLI flags), the chain (DAG) schema + executor,
                    and the wasm pack ABI
crates/packs/text   encodings, json, hash, gzip, diff, case/sort/escape tools
crates/packs/image  resize, crop, convert, compress, merge, EXIF, QR codes
crates/packs/crypto hmac, base32/58, uuid + password + random-byte generators
crates/packs/data   json↔yaml/toml/csv, xml, timestamps, url, regex, markdown
crates/cli          `toolkit` binary — links the packs natively
web/                Svelte app — catalog, tool pages, DAG chain builder;
                    loads the same packs as lazily-fetched wasm modules
chains/             community chain library (pure data, no code)
```

One tool implementation serves both frontends: the CLI links the Rust
directly; the browser fetches the pack compiled to WebAssembly, on first use,
through a tiny hand-written ABI (no codegen — see `crates/core/src/abi.rs`).
Chains are a versioned JSON DAG; the same file runs in the CLI, the web
builder, the shareable-URL encoding, and `chains/`.

**Toolchains**: tools declare typed, named input ports (most have one;
`image-merge` has `first` and `second`; a `multi` port like doc-merge's
`documents` accepts any number of connections, ordered), so they compose
into a DAG —
fan-out is allowed, every edge targets a specific port, and edges are
type-checked (with sanctioned runtime coercions like bytes→text-if-UTF-8).
A chain can declare **params** — named, typed knobs (`width`, `quality`)
that map onto node options — which makes a chain a first-class callable
unit: the CLI accepts them as `--set width=800`, the web builder renders
them as a settings form. Share a chain from the web builder: the URL encodes
the *definition*, never data.

**Streaming**: transducer-style tools (hash, base64, hex, URL, doc-merge —
marked `streaming` in the catalog) process input incrementally with
constant memory: hashing a multi-gigabyte file uses ~5 MB of RAM in the
CLI, and the browser feeds dropped files chunk-by-chunk via `file.stream()`
without ever loading them. Chains execute as a push-based dataflow — one
engine for both modes: streaming nodes transform chunk-by-chunk, tools
that inherently need the whole value (images, JSON) buffer only at their
own inputs, so `base64-decode | hash` streams end-to-end and memory is
bounded by the largest single buffered step, never the sum of
intermediates.

**Generators without ambient randomness**: uuid, password-gen and
random-bytes take their randomness through an explicit `entropy` input port
that the driver fills (CLI: OS RNG; web: `crypto.getRandomValues`). Tools
stay pure functions, the wasm sandbox keeps *zero* host imports, and the
entropy is visible in the request to anyone auditing — you can even wire
fixed bytes into the port for reproducible output.

**Supply-chain stance**: anything that touches user data is Rust with a
minimal, pure-Rust, pinned dependency set (vendorable via `cargo vendor`).
npm exists only for the web shell (svelte + vite, nothing else) and is
backstopped by the CSP above. Every tool must work on plain single-threaded
CPU; hardware acceleration may only ever be an additive fast path.

## Hosting

Any static file host works. Serve `web/dist` and send the same CSP as an HTTP
header (it's also in a `<meta>` tag, but a header covers more):

```
Content-Security-Policy: default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; connect-src 'self'; manifest-src 'self'; worker-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'
```

## Contributing

Adding a tool is one Rust file plus a registry line; adding a chain is one
JSON file. Start with [CONTRIBUTING.md](CONTRIBUTING.md); the concepts and
walkthroughs live in [docs/](docs/concepts.md).

## License

Apache-2.0
