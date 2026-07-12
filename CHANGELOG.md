# Changelog

All notable changes to this project are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Light theme: the site follows your OS preference and a header toggle
  pins either theme (persisted). Previously dark-only.
- Catalog: per-category accent colors, quick-jump chips, friendlier
  category names with one-line intros, `/` focuses search, and Enter
  opens the tool when the search has exactly one match.
- Tool pages link to related tools (shared-keyword neighbors).
- Tools can declare an example input in their manifest (53 do), and a
  CLI test runs every example with default options — a broken or stale
  example fails the build. Tool pages get a "Try an example" button;
  `toolkit info` prints a runnable example.
- `toolkit info --chain <name>` shows a chain's full definition (nodes
  with options, edges, params, declared inputs); `--json` dumps the raw
  chain JSON, so the remix loop is `info --chain X --json > my.json`,
  edit, `run-chain -f my.json`. A test keeps the dump re-runnable.
- Tab completion covers `info`: `--tool <TAB>` offers every tool name,
  `--chain <TAB>` offers the chain library (bash, zsh, fish), guarded
  by a test on the generated scripts.

### Changed
- `toolkit info` takes `--tool <name>` explicitly (was positional),
  making room for `--chain` without name-collision guesswork — a clean
  break like the v0.9.0 subcommand renames; the old form errors with
  the new usage.
- The unit converters' `from`/`to` options have sensible defaults
  (mi→km, kg→lb, gal→l, gib→mb, celsius→fahrenheit, px→pt) instead of
  being required — on the web the tools now run as soon as they have
  input, and in the CLI a bare `toolkit run-tool length-convert 5`
  works.
- The builder's tool picker is searchable (type to filter all tools),
  and its canvas instructions are collapsed behind a disclosure.
- Chain library cards show each pipeline as step chips instead of a
  mono text string; the mobile header no longer wraps awkwardly.

## [0.13.0] - 2026-07-12

### Added
- Six unit-conversion tools: `data-size-convert` (decimal and binary
  byte families), `length-convert`, `mass-convert`, `volume-convert`
  (metric and US customary), `temperature-convert` (celsius,
  fahrenheit, kelvin), and `px-convert` (px/pt/em/rem and physical
  units, with DPI and font-size context — defaults are the CSS
  reference pixel). One tool per category, so the from/to dropdowns
  and CLI completion only ever offer compatible units.
- `image-convert` can output webp — lossless only, since that's the
  only webp encoder that exists in pure Rust (expect files larger than
  jpeg; webp input was always supported).
- Updating is now one command: the installer saves a pinned copy of
  itself as `toolkit-update` next to the binary (a `.cmd` shim on
  Windows), refreshed on every update. `toolkit update` points there —
  the binary itself still contains no network code and spawns nothing.

### Fixed
- Updating failed with "Text file busy" while the binary was running
  (e.g. as a resident MCP server): the installer overwrote it in place.
  It now swaps via rename — atomic on Unix, so the installed binary
  stays intact until the new one lands; on Windows the old exe moves
  aside and is restored if the update fails partway.
- The installer's up-to-date check now probes the binary it would
  replace, not whatever `toolkit` is on PATH — a fresh install to a
  second directory no longer aborts as "already up to date".
- Intermittent "integrity check failed" in the web app after a deploy:
  the pinned wasm hashes were fetched at runtime, so the service worker
  could serve pins and packs from different deploys. The pins are now
  baked into the app bundle (atomically versioned with the code), and a
  mismatch refetches the pack past the caches once before failing —
  stale caches self-heal, real tampering still fails loudly.

## [0.12.1] - 2026-07-12

### Security
- The entire dependency tree is now audited: every crate is covered by a
  hand audit, a trusted publisher, or an imported third-party audit —
  `cargo vet` passes with zero exemptions, so any new or bumped
  dependency fails CI until it is consciously vetted.

### Changed
- Tool pages show a second CLI hint for multi-port tools: named ports
  take paths, and `<(command)` is a path, so a command's output can
  feed a port directly (`text-diff -i old=<(command) -i new=file`).

## [0.12.0] - 2026-07-11

### Added
- MCP `run-chain` tool: run a whole toolchain (pipe expression or inline
  chain JSON) in one call, so an agent doesn't shuttle intermediate data
  through separate tool calls. Supports chains with declared named inputs
  (an object of values) and multiple sinks (one output block per sink).

## [0.11.0] - 2026-07-11

### Added
- `toolkit mcp`: a Model Context Protocol server over stdio, exposing
  every tool to an LLM agent (no sockets; schemas generated from the
  manifests). See docs/mcp.md.

## [0.10.0] - 2026-07-11

### Added
- Ten developer tools: `number-base` (base 2–36 conversion), `json-diff`
  (structural diff of two JSON values), `json-schema-infer` (JSON Schema
  from a sample), `sql-format` (SQL beautifier), `http-status` (status
  code lookup), `jwt-verify` (HMAC signature verification),
  `text-to-binary`/`binary-to-text`, `slugify`, `text-stats` (streaming line/word/char/byte counts), and `lorem-ipsum`.

### Added
- Tool packs are now mechanically enforced pure: a CI clippy pass bans
  filesystem, network, subprocess, clock, and environment APIs, and every
  pack denies `unsafe`. Accidental impurity fails the build; deliberate
  impurity is conspicuous in review.
- Fuzz target for the chain schema (parse, validate, execute) and real
  image seeds for the image target; the harness now probes integer/float
  options at their bounds and covers color/JWT-shaped inputs.

### Fixed
- On Windows (no SIGPIPE), piping into a command that exits early no
  longer panics: a narrow panic hook exits quietly for broken-pipe print
  failures only, so genuine write errors still surface.

## [0.9.0] - 2026-07-11

### Added
- Chain `--set` completion now offers `node.option=` override keys (with
  their enum/bool values) alongside declared params — so completion is
  useful on every chain, not just the few with params.

### Changed
- Subcommands renamed for consistency: `tools` lists tools (like `chains`
  lists chains), `run-tool` runs a tool, `run-chain` runs a chain. The
  old names (`list`, `run`, `chain`) are gone — a clean break this early
  beats carrying aliases forever; the error suggests the new name.

### Fixed
- Piping into a command that exits early (`toolkit tools | head`, or a
  typo'd pipe target) panicked with "failed printing to stdout: Broken
  pipe". The CLI now restores default SIGPIPE handling on Unix and dies
  quietly like other tools.

## [0.8.0] - 2026-07-11

### Added
- The chain library is embedded in the CLI binary: `toolkit chains` and
  `toolkit chain -n <name>` work from any directory, no repo checkout
  needed. Files in `~/.config/toolkit/chains` override built-ins by name.

### Changed
- `--chains-dir` no longer defaults to `./chains`: a stray directory in
  the current working directory should never change what a chain name
  means. Pass it explicitly when working on a project's chain files.

### Changed
- `toolkit list` prints an aligned table (name, signature, description)
  with descriptions wrapped to the terminal width, instead of tab-separated
  lines that zigzagged with the signature column.

## [0.7.0] - 2026-07-11

### Added
- Chain completion: `toolkit chain -n <TAB>` offers chain names from the
  project and user libraries, and `--set <TAB>` offers the selected
  chain's declared params with their enum/bool values (bash, zsh, fish).

### Removed
- The `file-checksum` and `text-compare` chains: each wrapped a single
  tool and exposed nothing beyond it, so `toolkit run hash` and
  `toolkit run text-diff` do the same job directly (same options, same
  named inputs, same streaming).

## [0.6.0] - 2026-07-11

### Added
- Completions now cover tool options: `--set <TAB>` offers the selected
  tool's option keys, `--set key=<TAB>` its enum/bool values (bash, zsh,
  fish — the scripts call back into the binary, which knows the
  manifests). Hidden subcommands no longer leak into suggestions.

## [0.5.0] - 2026-07-11

### Added
- The install scripts refresh previously set-up shell completions on
  every update, so completions always match the installed version (they
  never create new shell config on their own).
- Homebrew tap and Scoop bucket, hosted in this repo (`Formula/`,
  `bucket/`) and regenerated by the release workflow with each tag's
  real checksums.

## [0.4.1] - 2026-07-11

### Fixed
- macOS release archives were 20-byte empty files in v0.2.0 through
  v0.4.0: the deterministic-tar flags are GNU-only, macOS runners run
  BSD tar, and without pipefail the failure was silent. The build now
  uses gtar on macOS, sanity-checks every archive, and the release job
  refuses duplicate asset hashes. Linux and Windows assets were never
  affected.

## [0.4.0] - 2026-07-11

### Added
- `toolkit completions <shell>`: shell completions for bash, zsh, fish,
  and PowerShell — including the tool names after `toolkit run` (bash,
  zsh, fish).

### Changed
- Dependency updates: getrandom 0.4, pulldown-cmark 0.13 (verified; the
  md-5/hmac digest-0.11 bumps are held until the whole RustCrypto family
  moves).

## [0.3.0] - 2026-07-11

### Added
- `toolkit run <tool> <value>`: single-input tools take the input directly
  as an argument (`toolkit run base64-encode hello`), repeatable for
  variable-arity ports; stdin and `-i` unchanged.

## [0.2.0] - 2026-07-11

### Added
- Community scaffolding: issue/PR templates and Dependabot for the cargo
  and github-actions ecosystems.
- `cargo vet` in CI: new dependencies fail until audited or consciously
  exempted; audits imported from Mozilla, Google, and Bytecode Alliance.
- Adversarial-input harness: every tool is exercised with arbitrary bytes
  in `cargo test` (no panics; chunk-split-independent streaming), plus
  cargo-fuzz targets per pack running weekly in CI.
- Web loader verifies each wasm pack against a pinned sha256
  (`wasm/integrity.json`) before instantiating it.
- The chain builder is fully keyboard-operable: Tab reaches nodes, ports,
  and connections; Enter connects/selects, Delete removes, arrow keys move
  nodes, Escape cancels — with ARIA labels throughout.
- PNG app icons (192/512 + apple-touch-icon) so installing to an iOS or
  Android home screen shows the lock icon instead of a blank tile.

- Chains can declare named inputs (`inputs` with port `binds`), so a chain
  can take several distinct values — the new `text-compare` chain diffs
  `old` against `new` (`toolkit chain -n text-compare -i old=a.txt -i
  new=b.txt`; the web builder shows one input panel per declared input).

- Windows installer: `irm …/install.ps1 | iex` downloads the latest
  release, verifies its SHA-256 checksum, installs to
  `%LOCALAPPDATA%\toolkit\bin`, and adds it to the user PATH. The user
  chain library now also resolves via `%USERPROFILE%` on Windows.
- Streaming downloads in the browser: when a chain runs over a large file,
  sink outputs can flow through the service worker straight into file
  downloads — a multi-GB result never sits in page memory.

### Fixed
- Tool pages went stale after the first run: editing the input or
  changing an option didn't re-run the tool (the auto-run effect only
  tracked the first empty→filled transition). Now every edit re-runs,
  debounced, as intended.
- `color-convert` panicked (instead of erroring) on hex notation
  containing multibyte characters, e.g. `#ééé` — found by the new fuzzer.

## [0.1.0] - 2026-07-10

### Added
- First release. 52 tools across four packs (text, image, crypto, data),
  runnable as a native CLI and in the browser via WebAssembly.
- DAG toolchains with declared parameters; 18 built-in chains.
- Named, multi-arity, and entropy input ports; end-to-end streaming.
- Client-side only: strict CSP, offline PWA, zero-network CLI.
- Reproducible builds with provenance attestation; five-platform binaries;
  `curl | sh` installer.

[Unreleased]: https://github.com/koundinyagoparaju/toolkit/compare/v0.13.0...HEAD
[0.13.0]: https://github.com/koundinyagoparaju/toolkit/compare/v0.12.1...v0.13.0
[0.12.1]: https://github.com/koundinyagoparaju/toolkit/compare/v0.12.0...v0.12.1
[0.12.0]: https://github.com/koundinyagoparaju/toolkit/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/koundinyagoparaju/toolkit/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/koundinyagoparaju/toolkit/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/koundinyagoparaju/toolkit/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/koundinyagoparaju/toolkit/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/koundinyagoparaju/toolkit/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/koundinyagoparaju/toolkit/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/koundinyagoparaju/toolkit/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/koundinyagoparaju/toolkit/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/koundinyagoparaju/toolkit/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/koundinyagoparaju/toolkit/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/koundinyagoparaju/toolkit/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/koundinyagoparaju/toolkit/releases/tag/v0.1.0
