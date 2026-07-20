# toolkit

**Everyday data tools that keep your data on your device.**

Encode, decode, inspect, convert, calculate, and transform data from your
terminal or browser. toolkit has no backend: the web app runs tools in
WebAssembly, and the CLI is a single native binary with no network client.

[Open the web app](https://koundinyagoparaju.github.io/toolkit/) ·
[Install the CLI](#install-the-cli) ·
[Browse the tools](#whats-in-the-box) ·
[Build a chain](#compose-tools-with-chains)

![Tool catalog](docs/images/catalog.png)

## Why toolkit?

- **Private by design.** JWTs, photos, API keys, and documents never leave
  your device.
- **Useful in two places.** The same Rust implementations power a friendly
  browser app and a fast command-line tool.
- **Composable.** Connect typed tools into reusable pipelines, from a short
  shell expression to a branching DAG.
- **Streaming where it matters.** Hash, encode, merge, and transform large
  files without loading the whole input into memory.
- **Built to be inspected.** No server, no hidden runtime downloads, pinned
  dependencies, verified WASM packs, and reproducible releases.

## Quick start

Use the browser without installing anything:

**[Launch toolkit →](https://koundinyagoparaju.github.io/toolkit/)**

Or install the CLI and run a tool:

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main/scripts/install.sh | sh

echo -n 'hello world' | toolkit run-tool base64-encode
# aGVsbG8gd29ybGQ=
```

Compose tools with pipe syntax:

```sh
echo -n "$JWT" | toolkit run-chain 'jwt-decode | json-format indent=4'
```

Process files without sending them anywhere:

```sh
toolkit run-chain -n image-web-ready \
  --set width=800 \
  -i photo.png \
  -o photo.jpg

toolkit run-tool hash -i backup.iso
```

## What's in the box

toolkit currently includes more than 90 tools across five packs:

| Pack | Examples |
|---|---|
| Text | Base64, hex, gzip, hashes, JWT inspection, diffs, case conversion, line sorting |
| Data | JSON/YAML/TOML/CSV, JSONPath, regex, timestamps, URLs, CIDR, cron, SQL formatting |
| Image | Resize, crop, convert, compress, merge, rotate, EXIF cleaning, QR codes |
| Crypto | HMAC, TOTP, Base32/58, certificate decoding, UUIDs, passwords, random bytes |
| Math | Calculator, statistics, prime factorization, combinatorics, percentages |

Run `toolkit tools` for the complete catalog, or browse it in the
[web app](https://koundinyagoparaju.github.io/toolkit/).

## Install the CLI

### Linux and macOS

```sh
curl -fsSL https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main/scripts/install.sh | sh
```

### Homebrew

```sh
brew tap koundinyagoparaju/toolkit https://github.com/koundinyagoparaju/toolkit
brew install koundinyagoparaju/toolkit/toolkit
```

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main/scripts/install.ps1 | iex
```

### Scoop

```powershell
scoop bucket add toolkit https://github.com/koundinyagoparaju/toolkit
scoop install toolkit
```

### Build from source

```sh
cargo build --release --locked -p toolkit-cli
# binary: target/release/toolkit
```

The install scripts download a release for your platform and verify its
SHA-256 checksum. They are intentionally short and live in this repository:
read [install.sh](scripts/install.sh) or [install.ps1](scripts/install.ps1),
or build from source if you prefer.

## Use the CLI

Inputs can be passed as an argument, piped through stdin, or read from a file:

```sh
toolkit tools
toolkit info --tool hash

toolkit run-tool base64-encode 'hello world'
echo -n 'hello' | toolkit run-tool base64-encode
toolkit run-tool image-resize --set width=800 -i input.png -o output.png
```

Tools with named ports accept `port=path` inputs:

```sh
toolkit run-tool image-merge \
  -i first=left.png \
  -i second=right.png \
  --set mode=horizontal \
  -o joined.png
```

Variable-arity ports accept repeated inputs in order:

```sh
toolkit run-tool doc-merge \
  -i first.txt \
  -i second.txt \
  -i third.txt \
  --set separator=$'\n---\n'
```

### Shell completions

Completions include tool names, chain names, option keys, and enum values:

```sh
# zsh
toolkit completions zsh > ~/.zsh/completions/_toolkit

# bash
toolkit completions bash > ~/.local/share/bash-completion/completions/toolkit

# fish
toolkit completions fish > ~/.config/fish/completions/toolkit.fish
```

```powershell
toolkit completions powershell > "$env:LOCALAPPDATA\toolkit\completions.ps1"
Add-Content $PROFILE '. "$env:LOCALAPPDATA\toolkit\completions.ps1"'
```

The installer refreshes completions when toolkit is updated. Direct installs
also include `toolkit-update`; Homebrew and Scoop installations update through
their package managers. The main `toolkit` binary never updates itself and
contains no network code.

## Compose tools with chains

A chain is a typed pipeline stored as portable JSON. Chains can be linear,
branch into several tools, join through named ports, expose parameters, and
produce multiple outputs.

For a quick pipeline, use pipe syntax:

```sh
echo "$JWT" | toolkit run-chain 'jwt-decode | json-format indent=4'
```

For reusable workflows, run a built-in or custom chain:

```sh
toolkit chains
toolkit info --chain image-web-ready
toolkit run-chain --name image-web-ready --set width=800 -i photo.png -o photo.jpg
toolkit run-chain --file my-chain.json -i input.txt
```

The built-in chain library is embedded in the binary. Personal chains can be
placed in `~/.config/toolkit/chains/`; a personal chain with the same name as
a built-in one takes precedence.

The web chain builder creates the same format and can encode a definition in a
shareable URL. Only the chain definition is included—never the input data.

![Chain builder showing two resize branches feeding an image merge](docs/images/builder.png)

See [Concepts](docs/concepts.md) for the type system and execution model, or
[Adding a chain](docs/adding-a-chain.md) for the JSON format.

## Use toolkit from an AI agent

toolkit can expose its tools through the
[Model Context Protocol](https://modelcontextprotocol.io) over stdio:

```sh
toolkit mcp
```

For Claude Code:

```sh
claude mcp add toolkit -- toolkit mcp
```

Clients that load every tool schema into context can use compact mode:

```sh
toolkit mcp --compact
```

Compact mode advertises `search-tools`, `run-tool`, and `run-chain` instead of
one schema per tool. See [MCP integration](docs/mcp.md) for details.

## How it works

```text
crates/core         typed values, manifests, options, chains, streaming, WASM ABI
crates/packs/text   encoding, formatting, hashing, compression, and text tools
crates/packs/data   structured data, network notation, regex, time, and format tools
crates/packs/image  image transforms, metadata, and QR tools
crates/packs/crypto signatures, certificates, OTP, IDs, and secure generators
crates/packs/math   arithmetic, statistics, number theory, and combinatorics
crates/cli          native CLI and MCP server
web/                Svelte catalog, tool runner, and visual chain builder
chains/             reusable chain definitions—pure data, no executable code
```

Each tool declares a manifest containing its typed input ports, output type,
options, description, and streaming capability. Those manifests drive the
CLI, web forms, MCP schemas, catalog, and chain validation. Adding a tool does
not require separate frontend implementations.

The CLI links the Rust packs directly. The browser loads the same packs,
compiled to WebAssembly, through a small hand-written ABI with no host imports.
WASM packs are fetched lazily, verified against hashes embedded in the app
bundle, and cached for offline use.

Chains run through a push-based dataflow engine. Streaming nodes pass chunks
on immediately; tools that need a complete value buffer only at their own
boundary. Memory use is therefore bounded by the largest buffered stage rather
than the total size of every intermediate value.

Random generators remain pure functions: the CLI supplies entropy from the OS,
and the browser supplies it through `crypto.getRandomValues`. The entropy is an
explicit input rather than an ambient capability inside a tool or WASM module.

For the byte-level protocol, execution rules, and release pipeline, read the
[architecture guide](docs/architecture.md).

## Trust and verification

toolkit is designed around the claim that sensitive data stays local:

- The web app is static and has no application server.
- Its CSP blocks connections to other origins.
- The CLI has no network client.
- WASM packs have zero host imports.
- Pack integrity is verified before instantiation.
- Dependencies are locked, audited with `cargo audit`, and reviewed with
  `cargo vet`.
- CI exercises every tool against arbitrary input and runs scheduled fuzzing.
- Release builds are reproducible and carry GitHub build-provenance
  attestations.

The browser's small JavaScript shell and service worker remain inside the trust
boundary because they see inputs before WASM. The native CLI bypasses that
layer entirely. The full threat model is documented in
[Architecture](docs/architecture.md#threat-model-briefly).

## Run the web app locally

Requirements: the pinned Rust toolchain, the `wasm32-unknown-unknown` target,
and Node.js/npm.

```sh
rustup target add wasm32-unknown-unknown
./scripts/build-web-assets.sh
cd web
npm ci
npm run dev
```

`build-web-assets.sh` compiles the five Rust packs to WASM, generates the tool
catalog, copies the chain library, and updates the embedded integrity hashes.

To produce a deployable static build:

```sh
cd web
npm run build
```

Any static host can serve `web/dist`. See [Architecture](docs/architecture.md)
for the recommended CSP and release-reproduction instructions.

## Contributing

Adding a tool is usually one Rust module plus one registry entry. Adding a
chain is one JSON file. Start with [CONTRIBUTING.md](CONTRIBUTING.md), then use
[Adding a tool](docs/adding-a-tool.md) or
[Adding a chain](docs/adding-a-chain.md) for a walkthrough.

## License

Apache-2.0
