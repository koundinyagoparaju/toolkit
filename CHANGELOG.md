# Changelog

All notable changes to this project are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Community scaffolding: issue/PR templates and Dependabot for the cargo
  and github-actions ecosystems.
- Fuzz targets for the hand-rolled decoders (base64, hex, url, gzip) and
  the image/QR/XML parsers.
- Web loader verifies each wasm pack against a sha256 pinned in
  `manifests.json` before instantiating it.

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
