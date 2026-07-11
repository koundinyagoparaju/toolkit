# Contributing

Contributions land as pull requests — that's not just process, it's the
security model: every tool and chain on the site got there through public
review, and the site never loads code from anywhere but this repo.

## Start here

- [docs/concepts.md](docs/concepts.md) — the mental model: values,
  coercions, ports, packs, chains, streaming, entropy, and the trust model
  everything serves.
- [docs/adding-a-tool.md](docs/adding-a-tool.md) — complete walkthrough:
  one Rust file + one registry line, with variants for options, multiple
  inputs, variable arity, randomness, and streaming.
- [docs/adding-a-chain.md](docs/adding-a-chain.md) — chains are pure JSON;
  schema, params, validation rules, and how to test.
- [docs/architecture.md](docs/architecture.md) — repo layout, the wasm
  pack ABI, the execution engine, drivers, and the release pipeline.

## Ground rules

- **Pure tools**: no filesystem, network, clock, or ambient randomness —
  a tool is a function of its inputs and options. Randomness comes through
  an entropy port.
- **Baseline path**: everything runs on plain single-threaded CPU;
  hardware acceleration may only ever be an additive fast path.
- **Dependencies**: pure-Rust only, well-maintained, genuinely needed —
  hand-roll trivial things (we hand-rolled hex and jwt-decode). New
  dependencies get extra scrutiny; `default-features = false` wherever
  possible. CI runs `cargo audit` (advisories) and `cargo vet`
  (supply-chain audits) on every PR — a new dependency fails CI until
  it's audited or consciously exempted in `supply-chain/config.toml`.
- **Input discipline**: be liberal in what you accept (strip whitespace,
  accept common variants), precise in your error messages.

## Development

```sh
cargo test                        # everything Rust
cargo run -p toolkit-cli -- list  # CLI against native packs
./scripts/build-web-assets.sh     # wasm packs + manifests for the web app
cd web && npm install && npm run dev
```

Before opening a PR: `cargo test`, `cargo fmt`, `cargo clippy`, and if you
touched the web app, `npm run build`. Note user-facing changes in
[CHANGELOG.md](CHANGELOG.md) under `## [Unreleased]`.

## Good first tools

Each of these is a single Rust file plus one registry line, pure by
construction, and backed by a well-maintained pure-Rust crate or a small
hand-roll. Follow [docs/adding-a-tool.md](docs/adding-a-tool.md):

- **Ascii85 / Base85** encode+decode (text → text)
- **Punycode** IDNA encode+decode (text → text)
- **Slugify** — text → URL-safe slug
- **Number base convert** — arbitrary radix 2–36 (text → text)
- **Roman numerals** encode+decode (text ↔ integer)
- **CSS/JS/HTML minify** (text → text)
- **JSON ⇄ CSV** flatten (json ↔ text)
- **Image → ASCII art** (image → text)
- **Barcode (Code128) generate** (text → image), pairing with the QR tools

Comment on an issue labelled `good first tool`, or open a
[tool request](.github/ISSUE_TEMPLATE/tool_request.md) with your idea.
