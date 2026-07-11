# Changelog

All notable changes to this project are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/koundinyagoparaju/toolkit/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/koundinyagoparaju/toolkit/releases/tag/v0.1.0
